<?php
/**
 * LabNAS - conector puente (relay con long-polling). Ver docs/PUENTE_PHP.md.
 *
 * El navegador pide /api/* -> el pedido queda en una cola en disco -> LabNAS,
 * que esta detras del NAT del laboratorio, lo recoge con /_labnas/poll, lo
 * ejecuta y devuelve la respuesta con /_labnas/respond.
 *
 * Compatible con PHP 7.0+: sin tipos nulables, void, funciones flecha, match,
 * asignacion con doble interrogacion ni funciones de PHP 8. Solo el nucleo.
 */

if (!defined('LB_ROOT')) {
    http_response_code(403);
    exit;
}

define('LB_POLL_BATCH', 8);
/** Cuerpos de pedido mas chicos que esto van dentro del poll (base64) */
define('LB_INLINE_BODY', 262144);
/** Archivos de cola huerfanos se borran despues de esto (segundos) */
define('LB_QUEUE_TTL', 120);
define('LB_NONCE_TTL', 300);
define('LB_CLOCK_SKEW', 60);

// ─── Utilidades ───

function lb_version()
{
    $f = LB_ROOT . '/VERSION';
    return is_file($f) ? trim(file_get_contents($f)) : 'dev';
}

/** Carpeta de datos (fuera de public_html), elegida en el instalador */
function lb_data_dir()
{
    $f = LB_ROOT . '/labnas-path.php';
    if (!is_file($f)) {
        return null;
    }
    $dir = include $f;
    return (is_string($dir) && is_dir($dir)) ? $dir : null;
}

function lb_read_json($file)
{
    if (!is_file($file)) {
        return null;
    }
    $data = json_decode(file_get_contents($file), true);
    return is_array($data) ? $data : null;
}

/** Escritura atomica: temporal + rename (el lector nunca ve un archivo a medias) */
function lb_write_file($file, $content)
{
    $tmp = $file . '.' . bin2hex(random_bytes(4)) . '.tmp';
    if (file_put_contents($tmp, $content) === false) {
        return false;
    }
    @chmod($tmp, 0600);
    return rename($tmp, $file);
}

function lb_config($dir)
{
    return lb_read_json($dir . '/config.json');
}

function lb_send_json($status, $data)
{
    http_response_code($status);
    header('Content-Type: application/json; charset=utf-8');
    header('Cache-Control: no-store');
    echo json_encode($data);
}

function lb_send_text($status, $text)
{
    http_response_code($status);
    header('Content-Type: text/plain; charset=utf-8');
    header('Cache-Control: no-store');
    echo $text;
}

/** Cabecera de la peticion (Apache a veces la deja como REDIRECT_HTTP_*) */
function lb_header($name)
{
    $key = 'HTTP_' . strtoupper(str_replace('-', '_', $name));
    if (isset($_SERVER[$key])) {
        return $_SERVER[$key];
    }
    if (isset($_SERVER['REDIRECT_' . $key])) {
        return $_SERVER['REDIRECT_' . $key];
    }
    if ($key === 'HTTP_CONTENT_TYPE' && isset($_SERVER['CONTENT_TYPE'])) {
        return $_SERVER['CONTENT_TYPE'];
    }
    return null;
}

/** "8M" -> bytes */
function lb_ini_bytes($value)
{
    $value = trim((string) $value);
    if ($value === '' || $value === '-1') {
        return -1;
    }
    $unit = strtolower(substr($value, -1));
    $num = (float) $value;
    switch ($unit) {
        case 'g':
            $num *= 1024;
            // no break
        case 'm':
            $num *= 1024;
            // no break
        case 'k':
            $num *= 1024;
    }
    return (int) $num;
}

function lb_limits()
{
    return array(
        'post_max_size' => lb_ini_bytes(ini_get('post_max_size')),
        'upload_max_filesize' => lb_ini_bytes(ini_get('upload_max_filesize')),
        'max_execution_time' => (int) ini_get('max_execution_time'),
        'memory_limit' => lb_ini_bytes(ini_get('memory_limit')),
    );
}

/** Segundos que un request puede esperar sin que PHP lo corte */
function lb_max_wait($wanted)
{
    @set_time_limit($wanted + 15);
    $limit = (int) ini_get('max_execution_time');
    if ($limit > 0 && $limit - 5 < $wanted) {
        return max(3, $limit - 5);
    }
    return $wanted;
}

function lb_queue($dir, $sub)
{
    return $dir . '/q/' . $sub;
}

function lb_valid_id($id)
{
    return is_string($id) && preg_match('/^[a-f0-9]{32}$/', $id) === 1;
}

/** Borra restos viejos de la cola y nonces vencidos */
function lb_cleanup($dir)
{
    $now = time();
    foreach (array('req', 'claimed', 'res') as $sub) {
        foreach ((array) glob(lb_queue($dir, $sub) . '/*') as $f) {
            if (is_file($f) && $now - filemtime($f) > LB_QUEUE_TTL) {
                @unlink($f);
            }
        }
    }
    foreach ((array) glob($dir . '/nonces/*') as $f) {
        if (is_file($f) && $now - filemtime($f) > LB_NONCE_TTL) {
            @unlink($f);
        }
    }
}

// ─── Entrada ───

function lb_main()
{
    $path = parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH);
    if (!is_string($path)) {
        $path = '/';
    }

    if (strpos($path, '/_labnas/') === 0) {
        lb_ops(substr($path, strlen('/_labnas/')));
        return;
    }
    if ($path === '/api' || strpos($path, '/api/') === 0) {
        lb_client($path);
        return;
    }
    lb_static($path);
}

// ─── Frontend estatico (web/) ───

function lb_static($path)
{
    $web = LB_ROOT . '/web';
    if (!is_dir($web)) {
        lb_send_text(503, "Conector de LabNAS sin frontend.\nDescarga el paquete completo desde LabNAS > Configuracion > Acceso remoto.");
        return;
    }
    $rel = ltrim(rawurldecode($path), '/');
    $file = realpath($web . '/' . $rel);
    $webReal = realpath($web);
    $isAsset = $rel !== '' && $file !== false && is_file($file) && strpos($file, $webReal . DIRECTORY_SEPARATOR) === 0;
    if (!$isAsset) {
        $file = $webReal . '/index.html';
        if (!is_file($file)) {
            lb_send_text(404, 'No encontrado');
            return;
        }
    }

    $types = array(
        'html' => 'text/html; charset=utf-8', 'js' => 'text/javascript; charset=utf-8',
        'css' => 'text/css; charset=utf-8', 'svg' => 'image/svg+xml', 'png' => 'image/png',
        'jpg' => 'image/jpeg', 'ico' => 'image/x-icon', 'json' => 'application/json',
        'webmanifest' => 'application/manifest+json', 'woff2' => 'font/woff2', 'woff' => 'font/woff',
        'txt' => 'text/plain; charset=utf-8', 'map' => 'application/json',
    );
    $ext = strtolower(pathinfo($file, PATHINFO_EXTENSION));
    header('Content-Type: ' . (isset($types[$ext]) ? $types[$ext] : 'application/octet-stream'));
    // Los assets de Vite llevan hash en el nombre: cache larga. index.html siempre fresco.
    if (strpos($rel, 'assets/') === 0 && $isAsset) {
        header('Cache-Control: public, max-age=31536000, immutable');
    } else {
        header('Cache-Control: no-cache');
    }
    header('Content-Length: ' . filesize($file));
    header('X-Content-Type-Options: nosniff');
    readfile($file);
}

// ─── Navegador -> cola -> LabNAS ───

/** Cabeceras del navegador que se reenvian a LabNAS */
function lb_forward_headers()
{
    $names = array('Authorization', 'Cookie', 'X-LabNAS-CSRF', 'Origin', 'Content-Type', 'Range', 'If-None-Match', 'If-Modified-Since', 'If-Range', 'Accept', 'Accept-Language');
    $out = array();
    foreach ($names as $n) {
        $v = lb_header($n);
        if ($v !== null && $v !== '') {
            $out[] = array($n, $v);
        }
    }
    return $out;
}

/** Campos de $_POST/$_FILES aplanados como name[] (un nivel basta para LabNAS) */
function lb_flatten($name, $value, &$out)
{
    if (is_array($value)) {
        foreach ($value as $k => $v) {
            lb_flatten($name . '[' . (is_int($k) ? '' : $k) . ']', $v, $out);
        }
    } else {
        $out[] = array($name, (string) $value);
    }
}

/**
 * Rearma el multipart en $file (primero los campos, luego los archivos: LabNAS
 * lee la carpeta destino antes que el archivo). Devuelve array(largo, content-type)
 * o un texto de error.
 */
function lb_rebuild_multipart($file)
{
    $boundary = '----labnas' . bin2hex(random_bytes(12));
    $fh = @fopen($file . '.part', 'wb');
    if (!$fh) {
        return 'Sin espacio en el hosting para el pedido';
    }
    $fields = array();
    foreach ($_POST as $k => $v) {
        lb_flatten($k, $v, $fields);
    }
    foreach ($fields as $f) {
        fwrite($fh, '--' . $boundary . "\r\nContent-Disposition: form-data; name=\"" . str_replace('"', '%22', $f[0]) . "\"\r\n\r\n" . $f[1] . "\r\n");
    }
    foreach ($_FILES as $field => $info) {
        $names = (array) $info['name'];
        $tmps = (array) $info['tmp_name'];
        $errors = (array) $info['error'];
        $types = (array) $info['type'];
        $suffix = is_array($info['name']) ? '[]' : '';
        foreach ($names as $i => $fname) {
            $err = isset($errors[$i]) ? (int) $errors[$i] : UPLOAD_ERR_NO_FILE;
            if ($err === UPLOAD_ERR_INI_SIZE || $err === UPLOAD_ERR_FORM_SIZE) {
                fclose($fh);
                @unlink($file . '.part');
                return 'El archivo supera el limite del hosting (' . ini_get('upload_max_filesize') . ')';
            }
            if ($err !== UPLOAD_ERR_OK || !is_uploaded_file($tmps[$i])) {
                continue;
            }
            $type = isset($types[$i]) && $types[$i] !== '' ? $types[$i] : 'application/octet-stream';
            fwrite($fh, '--' . $boundary . "\r\nContent-Disposition: form-data; name=\"" . str_replace('"', '%22', $field . $suffix)
                . "\"; filename=\"" . str_replace(array('"', "\r", "\n"), array('%22', '', ''), (string) $fname) . "\"\r\nContent-Type: "
                . str_replace(array("\r", "\n"), '', $type) . "\r\n\r\n");
            $in = fopen($tmps[$i], 'rb');
            stream_copy_to_stream($in, $fh);
            fclose($in);
            fwrite($fh, "\r\n");
        }
    }
    fwrite($fh, '--' . $boundary . "--\r\n");
    fclose($fh);
    if (!rename($file . '.part', $file)) {
        return 'Sin espacio en el hosting para el pedido';
    }
    @chmod($file, 0600);
    return array(filesize($file), 'multipart/form-data; boundary=' . $boundary);
}

function lb_client($path)
{
    $dir = lb_data_dir();
    $cfg = $dir ? lb_config($dir) : null;
    if (!$cfg) {
        lb_send_text(503, 'Este puente aun no esta enlazado con LabNAS');
        return;
    }

    $limits = lb_limits();
    $len = isset($_SERVER['CONTENT_LENGTH']) ? (int) $_SERVER['CONTENT_LENGTH'] : 0;
    if ($limits['post_max_size'] > 0 && $len > $limits['post_max_size']) {
        lb_send_text(413, 'El archivo supera el limite del hosting (' . ini_get('post_max_size') . ')');
        return;
    }
    $id = bin2hex(random_bytes(16));
    $req = lb_queue($dir, 'req');
    $bodyFile = $req . '/' . $id . '.body';
    $headers = lb_forward_headers();

    // PHP no expone el cuerpo crudo de un multipart/form-data: se rearma desde $_POST/$_FILES
    $ctype = (string) lb_header('Content-Type');
    if (stripos($ctype, 'multipart/form-data') === 0) {
        $rebuilt = lb_rebuild_multipart($bodyFile);
        if (is_string($rebuilt)) {
            lb_send_text(413, $rebuilt);
            return;
        }
        $bodyLen = $rebuilt[0];
        foreach ($headers as $i => $h) {
            if (strtolower($h[0]) === 'content-type') {
                $headers[$i][1] = $rebuilt[1];
            }
        }
    } else {
        $body = file_get_contents('php://input');
        if ($body === false) {
            $body = '';
        }
        $bodyLen = strlen($body);
        if ($body !== '' && !lb_write_file($bodyFile, $body)) {
            lb_send_text(507, 'Sin espacio en el hosting para el pedido');
            return;
        }
        unset($body);
    }

    $meta = array(
        'id' => $id,
        'method' => $_SERVER['REQUEST_METHOD'],
        'path' => $path,
        'query' => isset($_SERVER['QUERY_STRING']) ? $_SERVER['QUERY_STRING'] : '',
        'headers' => $headers,
        'client_ip' => isset($_SERVER['REMOTE_ADDR']) ? $_SERVER['REMOTE_ADDR'] : '',
        'body_len' => $bodyLen,
        'created' => time(),
    );
    // El .json va al final: el poller solo ve pedidos completos
    lb_write_file($req . '/' . $id . '.json', json_encode($meta));

    ignore_user_abort(false);
    $wait = lb_max_wait(25);
    $deadline = microtime(true) + $wait;
    $res = lb_queue($dir, 'res');
    while (microtime(true) < $deadline) {
        clearstatcache();
        if (is_file($res . '/' . $id . '.json')) {
            lb_output_response($res, $id);
            return;
        }
        if (connection_aborted()) {
            break;
        }
        usleep(100000);
    }

    // Nadie lo atendio: retirar el pedido
    foreach (array('req', 'claimed') as $sub) {
        @unlink(lb_queue($dir, $sub) . '/' . $id . '.json');
        @unlink(lb_queue($dir, $sub) . '/' . $id . '.body');
    }
    lb_send_text(504, 'LabNAS no responde. Revisa que este encendido y con internet.');
}

function lb_output_response($res, $id)
{
    $meta = lb_read_json($res . '/' . $id . '.json');
    if (!$meta) {
        lb_send_text(502, 'Respuesta invalida de LabNAS');
        return;
    }
    http_response_code(isset($meta['status']) ? (int) $meta['status'] : 502);
    $skip = array('content-length', 'transfer-encoding', 'connection', 'keep-alive');
    if (isset($meta['headers']) && is_array($meta['headers'])) {
        foreach ($meta['headers'] as $h) {
            if (!is_array($h) || count($h) !== 2) {
                continue;
            }
            $name = (string) $h[0];
            $value = str_replace(array("\r", "\n"), '', (string) $h[1]);
            if (in_array(strtolower($name), $skip, true) || preg_match('/^[A-Za-z0-9-]+$/', $name) !== 1) {
                continue;
            }
            header($name . ': ' . $value, false);
        }
    }
    header('X-Accel-Buffering: no');
    if (isset($meta['length']) && is_int($meta['length']) && $_SERVER['REQUEST_METHOD'] !== 'HEAD') {
        header('Content-Length: ' . $meta['length']);
    }

    // Las partes se entregan apenas llegan (archivos grandes, video)
    @ini_set('zlib.output_compression', 'Off');
    while (ob_get_level() > 0) {
        @ob_end_flush();
    }
    @set_time_limit(0);
    ignore_user_abort(true);

    $seq = 0;
    $partDeadline = time() + 90;
    while (true) {
        clearstatcache();
        $pf = $res . '/' . $id . '.p' . $seq;
        if (is_file($pf)) {
            readfile($pf);
            @unlink($pf);
            flush();
            // Descarga larga: que la limpieza de la cola (120 s) no borre el pedido
            @touch($res . '/' . $id . '.json');
            $seq++;
            $partDeadline = time() + 90;
            continue;
        }
        $end = $res . '/' . $id . '.end';
        if (is_file($end) && $seq > (int) file_get_contents($end)) {
            break;
        }
        if (connection_aborted() || time() > $partDeadline) {
            // Avisa a LabNAS que deje de enviar
            @touch($res . '/' . $id . '.gone');
            break;
        }
        usleep(50000);
    }
    @unlink($res . '/' . $id . '.json');
    @unlink($res . '/' . $id . '.end');
    foreach ((array) glob($res . '/' . $id . '.p*') as $f) {
        @unlink($f);
    }
}

// ─── Protocolo con LabNAS (/_labnas/*) ───

/** Verifica la firma HMAC de una llamada de LabNAS. Devuelve null o el texto del error. */
function lb_verify($dir, $cfg, $op, $body)
{
    $node = lb_header('X-Node');
    $ts = lb_header('X-Ts');
    $nonce = lb_header('X-Nonce');
    $sig = lb_header('X-Sig');
    if ($node === null || $ts === null || $nonce === null || $sig === null) {
        return 'Faltan cabeceras de firma';
    }
    if (!hash_equals((string) $cfg['node_id'], (string) $node)) {
        return 'Nodo desconocido';
    }
    if (abs(time() - (int) $ts) > LB_CLOCK_SKEW) {
        return 'Reloj desfasado (revisa la hora de LabNAS y del hosting)';
    }
    if (preg_match('/^[a-f0-9]{32}$/', $nonce) !== 1) {
        return 'Nonce invalido';
    }
    $query = isset($_SERVER['QUERY_STRING']) ? $_SERVER['QUERY_STRING'] : '';
    $data = $op . "\n" . $ts . "\n" . $nonce . "\n" . hash('sha256', $body) . "\n" . $query;
    $expected = hash_hmac('sha256', $data, hex2bin($cfg['key']));
    if (!hash_equals($expected, strtolower((string) $sig))) {
        return 'Firma invalida';
    }
    $nf = $dir . '/nonces/' . $nonce;
    if (file_exists($nf)) {
        return 'Nonce repetido';
    }
    @touch($nf);
    return null;
}

function lb_ops($op)
{
    if ($_SERVER['REQUEST_METHOD'] !== 'POST') {
        lb_send_text(405, 'Solo POST');
        return;
    }
    $dir = lb_data_dir();
    if (!$dir) {
        lb_send_text(503, 'Conector sin instalar: abre instalar.php');
        return;
    }
    $body = file_get_contents('php://input');
    if ($body === false) {
        $body = '';
    }

    if ($op === 'register') {
        lb_op_register($dir, $body);
        return;
    }

    $cfg = lb_config($dir);
    if (!$cfg) {
        lb_send_text(409, 'Conector sin enlazar');
        return;
    }
    $err = lb_verify($dir, $cfg, $op, $body);
    if ($err !== null) {
        lb_send_text(401, $err);
        return;
    }

    switch ($op) {
        case 'status':
            lb_cleanup($dir);
            lb_send_json(200, array(
                'version' => lb_version(),
                'php' => PHP_VERSION,
                'server' => isset($_SERVER['SERVER_SOFTWARE']) ? $_SERVER['SERVER_SOFTWARE'] : '',
                'limits' => lb_limits(),
                'disk_free' => @disk_free_space($dir),
                'time' => time(),
            ));
            return;
        case 'poll':
            lb_op_poll($dir);
            return;
        case 'body':
            lb_op_body($dir);
            return;
        case 'respond':
            lb_op_respond($dir, $body);
            return;
        case 'update_begin':
            lb_op_update_begin($dir, $body);
            return;
        case 'update_file':
            lb_op_update_file($dir, $body);
            return;
        case 'update_commit':
            lb_op_update_commit($dir);
            return;
        default:
            lb_send_text(404, 'Operacion desconocida');
    }
}

/** Enlace inicial: lo autoriza el codigo de instalacion (un solo uso, 30 min) */
function lb_op_register($dir, $body)
{
    if (lb_config($dir)) {
        lb_send_text(409, 'El puente ya esta enlazado. Para enlazarlo de nuevo borra config.json en la carpeta de datos.');
        return;
    }
    $install = lb_read_json($dir . '/install.json');
    if (!$install || time() > (int) $install['expires']) {
        lb_send_text(410, 'Codigo vencido o inexistente: genera otro en instalar.php');
        return;
    }
    $req = json_decode($body, true);
    if (!is_array($req) || !isset($req['code'], $req['node_id'], $req['key'])) {
        lb_send_text(400, 'Pedido invalido');
        return;
    }
    $code = strtoupper(preg_replace('/[^A-Za-z0-9]/', '', (string) $req['code']));
    if (!hash_equals((string) $install['hash'], hash('sha256', $code))) {
        $install['attempts'] = (isset($install['attempts']) ? (int) $install['attempts'] : 0) + 1;
        if ($install['attempts'] >= 10) {
            @unlink($dir . '/install.json');
        } else {
            lb_write_file($dir . '/install.json', json_encode($install));
        }
        lb_send_text(403, 'Codigo incorrecto');
        return;
    }
    if (preg_match('/^[a-f0-9]{32}$/', (string) $req['node_id']) !== 1 || preg_match('/^[a-f0-9]{64}$/', (string) $req['key']) !== 1) {
        lb_send_text(400, 'Clave invalida');
        return;
    }
    lb_write_file($dir . '/config.json', json_encode(array(
        'node_id' => $req['node_id'],
        'key' => $req['key'],
        'linked_at' => time(),
    )));
    @unlink($dir . '/install.json');
    lb_send_json(200, array('ok' => true, 'version' => lb_version(), 'limits' => lb_limits()));
}

/** Espera pedidos (long-poll) y entrega hasta LB_POLL_BATCH */
function lb_op_poll($dir)
{
    lb_cleanup($dir);
    $wait = lb_max_wait(20);
    $deadline = microtime(true) + $wait;
    $req = lb_queue($dir, 'req');
    $claimed = lb_queue($dir, 'claimed');
    $out = array();

    while (true) {
        clearstatcache();
        $files = glob($req . '/*.json');
        if (is_array($files) && count($files) > 0) {
            usort($files, function ($a, $b) {
                return filemtime($a) - filemtime($b);
            });
            foreach ($files as $f) {
                $id = basename($f, '.json');
                // rename es atomico: si otro poller lo tomo primero, falla
                if (!lb_valid_id($id) || !@rename($f, $claimed . '/' . $id . '.json')) {
                    continue;
                }
                if (is_file($req . '/' . $id . '.body')) {
                    @rename($req . '/' . $id . '.body', $claimed . '/' . $id . '.body');
                }
                $meta = lb_read_json($claimed . '/' . $id . '.json');
                if (!$meta) {
                    continue;
                }
                $meta['body_b64'] = null;
                $bf = $claimed . '/' . $id . '.body';
                if (is_file($bf) && filesize($bf) <= LB_INLINE_BODY) {
                    $meta['body_b64'] = base64_encode(file_get_contents($bf));
                }
                $out[] = $meta;
                if (count($out) >= LB_POLL_BATCH) {
                    break;
                }
            }
        }
        if (count($out) > 0 || microtime(true) >= $deadline || connection_aborted()) {
            break;
        }
        usleep(100000);
    }
    lb_send_json(200, array('requests' => $out));
}

/** Cuerpo completo de un pedido grande (subidas) */
function lb_op_body($dir)
{
    $id = isset($_GET['id']) ? $_GET['id'] : '';
    $f = lb_queue($dir, 'claimed') . '/' . $id . '.body';
    if (!lb_valid_id($id) || !is_file($f)) {
        lb_send_text(404, 'Sin cuerpo');
        return;
    }
    header('Content-Type: application/octet-stream');
    header('Content-Length: ' . filesize($f));
    readfile($f);
}

/**
 * Respuesta de LabNAS, en una o varias partes: ?id=&seq=N&final=0|1, y en la
 * parte 0 tambien ?meta= (base64url JSON: status, headers, length). Cuerpo crudo.
 */
function lb_op_respond($dir, $body)
{
    $id = isset($_GET['id']) ? $_GET['id'] : '';
    if (!lb_valid_id($id)) {
        lb_send_text(400, 'id invalido');
        return;
    }
    $seq = isset($_GET['seq']) ? (int) $_GET['seq'] : 0;
    $final = !isset($_GET['final']) || $_GET['final'] === '1';
    $claimed = lb_queue($dir, 'claimed');
    $res = lb_queue($dir, 'res');

    if (is_file($res . '/' . $id . '.gone')) {
        lb_send_text(410, 'El navegador cerro la descarga');
        return;
    }
    if ($seq === 0) {
        if (!is_file($claimed . '/' . $id . '.json')) {
            // El navegador se canso de esperar
            lb_send_text(410, 'El pedido ya no existe');
            return;
        }
        $metaRaw = isset($_GET['meta']) ? base64_decode(strtr($_GET['meta'], '-_', '+/')) : false;
        $meta = $metaRaw !== false ? json_decode($metaRaw, true) : null;
        if (!is_array($meta) || !isset($meta['status'])) {
            lb_send_text(400, 'meta invalida');
            return;
        }
    } else {
        if (!is_file($res . '/' . $id . '.json')) {
            lb_send_text(410, 'El pedido ya no existe');
            return;
        }
        // Contrapresion: si el navegador va lento, LabNAS espera antes de mandar mas
        $pending = glob($res . '/' . $id . '.p*');
        if (is_array($pending) && count($pending) >= 6) {
            lb_send_text(429, 'Espera');
            return;
        }
    }

    if (!lb_write_file($res . '/' . $id . '.p' . $seq, $body)) {
        lb_send_text(507, 'Sin espacio en el hosting');
        return;
    }
    if ($seq === 0) {
        lb_write_file($res . '/' . $id . '.json', json_encode($meta));
        @unlink($claimed . '/' . $id . '.json');
        @unlink($claimed . '/' . $id . '.body');
    }
    if ($final) {
        lb_write_file($res . '/' . $id . '.end', (string) $seq);
    }
    lb_send_json(200, array('ok' => true));
}

// ─── Autoactualizacion (la envia LabNAS; todo firmado con la clave del enlace) ───
//
// Se actualizan app/, web/, instalar.php y VERSION. index.php y .htaccess nunca:
// si una actualizacion los rompiera, no habria forma de arreglarlo a distancia.

function lb_update_path_ok($path)
{
    if (!is_string($path) || $path === '' || strlen($path) > 300) {
        return false;
    }
    if (strpos($path, '..') !== false || $path[0] === '/' || strpos($path, '\\') !== false) {
        return false;
    }
    if (preg_match('#^[A-Za-z0-9._/-]+$#', $path) !== 1) {
        return false;
    }
    return $path === 'instalar.php' || $path === 'VERSION' || strpos($path, 'app/') === 0 || strpos($path, 'web/') === 0;
}

function lb_rmtree($dir)
{
    if (!is_dir($dir)) {
        return;
    }
    foreach ((array) scandir($dir) as $e) {
        if ($e === '.' || $e === '..') {
            continue;
        }
        $p = $dir . '/' . $e;
        if (is_dir($p) && !is_link($p)) {
            lb_rmtree($p);
        } else {
            @unlink($p);
        }
    }
    @rmdir($dir);
}

/** Manifiesto: {version, files: [{path, sha256, size}]} */
function lb_op_update_begin($dir, $body)
{
    $m = json_decode($body, true);
    if (!is_array($m) || !isset($m['version'], $m['files']) || !is_array($m['files']) || count($m['files']) === 0) {
        lb_send_text(400, 'Manifiesto invalido');
        return;
    }
    foreach ($m['files'] as $f) {
        if (!is_array($f) || !isset($f['path'], $f['sha256'], $f['size']) || !lb_update_path_ok($f['path'])
            || preg_match('/^[a-f0-9]{64}$/', (string) $f['sha256']) !== 1) {
            lb_send_text(400, 'Archivo invalido en el manifiesto');
            return;
        }
    }
    if (preg_match('/^[0-9A-Za-z.+-]{1,40}$/', (string) $m['version']) !== 1) {
        lb_send_text(400, 'Version invalida');
        return;
    }
    $staging = LB_ROOT . '/.update-staging';
    lb_rmtree($staging);
    if (!@mkdir($staging, 0755, true)) {
        lb_send_text(500, 'No se pudo crear la carpeta de actualizacion (permisos)');
        return;
    }
    lb_write_file($dir . '/update.json', json_encode($m));
    lb_send_json(200, array('ok' => true));
}

function lb_update_manifest_entry($dir, $path)
{
    $m = lb_read_json($dir . '/update.json');
    if (!$m) {
        return null;
    }
    foreach ($m['files'] as $f) {
        if ($f['path'] === $path) {
            return $f;
        }
    }
    return null;
}

function lb_op_update_file($dir, $body)
{
    $path = isset($_GET['path']) ? (string) $_GET['path'] : '';
    $entry = lb_update_path_ok($path) ? lb_update_manifest_entry($dir, $path) : null;
    if (!$entry) {
        lb_send_text(400, 'Archivo fuera del manifiesto');
        return;
    }
    if (strlen($body) !== (int) $entry['size'] || !hash_equals((string) $entry['sha256'], hash('sha256', $body))) {
        lb_send_text(400, 'El archivo no coincide con el manifiesto');
        return;
    }
    $target = LB_ROOT . '/.update-staging/' . $path;
    $parent = dirname($target);
    if (!is_dir($parent) && !@mkdir($parent, 0755, true)) {
        lb_send_text(500, 'No se pudo crear ' . dirname($path));
        return;
    }
    if (file_put_contents($target, $body) === false) {
        lb_send_text(507, 'Sin espacio en el hosting');
        return;
    }
    @chmod($target, 0644);
    lb_send_json(200, array('ok' => true));
}

/** Verifica todo y reemplaza con renames; si algo falla, vuelve atras */
function lb_op_update_commit($dir)
{
    $m = lb_read_json($dir . '/update.json');
    $staging = LB_ROOT . '/.update-staging';
    if (!$m || !is_dir($staging)) {
        lb_send_text(409, 'No hay actualizacion en curso');
        return;
    }
    foreach ($m['files'] as $f) {
        $p = $staging . '/' . $f['path'];
        if (!is_file($p) || !hash_equals((string) $f['sha256'], hash_file('sha256', $p))) {
            lb_send_text(400, 'Falta o no coincide: ' . $f['path']);
            return;
        }
    }
    $old = LB_ROOT . '/.update-old';
    lb_rmtree($old);
    @mkdir($old, 0755, true);
    $names = array('app', 'web', 'instalar.php', 'VERSION');
    $moved = array();
    $ok = true;
    foreach ($names as $n) {
        if (!file_exists($staging . '/' . $n)) {
            continue;
        }
        if (file_exists(LB_ROOT . '/' . $n) && !@rename(LB_ROOT . '/' . $n, $old . '/' . $n)) {
            $ok = false;
            break;
        }
        $moved[] = $n;
        if (!@rename($staging . '/' . $n, LB_ROOT . '/' . $n)) {
            $ok = false;
            break;
        }
    }
    if (!$ok) {
        // Vuelta atras
        foreach (array_reverse($moved) as $n) {
            if (file_exists(LB_ROOT . '/' . $n) && file_exists($old . '/' . $n)) {
                lb_rmtree(LB_ROOT . '/' . $n);
                @unlink(LB_ROOT . '/' . $n);
            }
            if (file_exists($old . '/' . $n)) {
                @rename($old . '/' . $n, LB_ROOT . '/' . $n);
            }
        }
        lb_send_text(500, 'No se pudo reemplazar el conector (permisos); se dejo la version anterior');
        return;
    }
    lb_rmtree($staging);
    @unlink($dir . '/update.json');
    if (function_exists('opcache_reset')) {
        @opcache_reset();
    }
    lb_send_json(200, array('ok' => true, 'version' => $m['version']));
}
