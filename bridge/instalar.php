<?php
/**
 * LabNAS - instalador del conector puente. Compatible con PHP 7.0+.
 *
 * 1. Elige la carpeta de datos (fuera de public_html).
 * 2. Muestra un codigo de un solo uso (30 min) que se pega en LabNAS.
 * Una vez enlazado, este archivo ya no hace nada.
 */
define('LB_ROOT', __DIR__);
require __DIR__ . '/app/bridge.php';

header('Content-Type: text/html; charset=utf-8');
header('Cache-Control: no-store');
header('X-Frame-Options: DENY');

function lb_h($s)
{
    return htmlspecialchars((string) $s, ENT_QUOTES, 'UTF-8');
}

function lb_fmt_bytes($n)
{
    if ($n < 0) {
        return 'sin limite';
    }
    $u = array('B', 'KB', 'MB', 'GB');
    $i = 0;
    while ($n >= 1024 && $i < 3) {
        $n /= 1024;
        $i++;
    }
    return round($n, 1) . ' ' . $u[$i];
}

/** Codigo legible: sin 0/O ni 1/I/L */
function lb_new_code()
{
    $alphabet = 'ABCDEFGHJKMNPQRSTUVWXYZ23456789';
    $code = '';
    for ($i = 0; $i < 16; $i++) {
        $code .= $alphabet[random_int(0, strlen($alphabet) - 1)];
    }
    return $code;
}

/** ¿$path queda dentro de la raiz web? (ahi cualquiera podria descargar las claves) */
function lb_inside_webroot($path)
{
    $real = realpath($path);
    $roots = array(realpath(__DIR__));
    if (!empty($_SERVER['DOCUMENT_ROOT'])) {
        $roots[] = realpath($_SERVER['DOCUMENT_ROOT']);
    }
    foreach ($roots as $r) {
        if ($r !== false && $real !== false && ($real === $r || strpos($real, $r . DIRECTORY_SEPARATOR) === 0)) {
            return true;
        }
    }
    return false;
}

function lb_prepare_dir($dir)
{
    if (!is_dir($dir) && !@mkdir($dir, 0700, true)) {
        return 'No se pudo crear la carpeta (revisa permisos o la ruta)';
    }
    if (lb_inside_webroot($dir)) {
        return 'La carpeta queda dentro de la parte publica de la web: elige una fuera de public_html';
    }
    foreach (array('q', 'q/req', 'q/claimed', 'q/res', 'nonces') as $sub) {
        if (!is_dir($dir . '/' . $sub) && !@mkdir($dir . '/' . $sub, 0700, true)) {
            return 'No se pudo crear ' . $sub;
        }
    }
    if (!is_writable($dir)) {
        return 'La carpeta no se puede escribir';
    }
    // Defensa extra por si algun dia queda accesible
    @file_put_contents($dir . '/.htaccess', "Require all denied\nDeny from all\n");
    $pointer = "<?php\n// Carpeta de datos del conector LabNAS (generado por instalar.php)\nreturn " . var_export(realpath($dir), true) . ";\n";
    if (!lb_write_file(LB_ROOT . '/labnas-path.php', $pointer)) {
        return 'No se pudo escribir labnas-path.php junto a index.php';
    }
    return null;
}

$dir = lb_data_dir();
$linked = $dir && lb_config($dir);
$error = null;
$code = null;

if (!$linked && $_SERVER['REQUEST_METHOD'] === 'POST') {
    $wanted = isset($_POST['data_dir']) ? trim($_POST['data_dir']) : '';
    if ($wanted === '') {
        $error = 'Indica la carpeta de datos';
    } else {
        $error = lb_prepare_dir($wanted);
    }
    if ($error === null) {
        $dir = lb_data_dir();
        $code = lb_new_code();
        lb_write_file($dir . '/install.json', json_encode(array(
            'hash' => hash('sha256', $code),
            'expires' => time() + 1800,
            'attempts' => 0,
        )));
    }
}

$default = $dir ? $dir : dirname(__DIR__) . DIRECTORY_SEPARATOR . 'labnas-data';
$limits = lb_limits();
$okPhp = version_compare(PHP_VERSION, '7.0.0', '>=');
$https = (!empty($_SERVER['HTTPS']) && $_SERVER['HTTPS'] !== 'off') || (isset($_SERVER['HTTP_X_FORWARDED_PROTO']) && $_SERVER['HTTP_X_FORWARDED_PROTO'] === 'https');
$host = isset($_SERVER['HTTP_HOST']) ? $_SERVER['HTTP_HOST'] : 'este-sitio';
$url = ($https ? 'https://' : 'http://') . $host;
?>
<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Instalar conector LabNAS</title>
<style>
  body { font-family: system-ui, sans-serif; background: #282a36; color: #f8f8f2; margin: 0; padding: 24px 16px; }
  main { max-width: 640px; margin: 0 auto; }
  h1 { font-size: 1.4rem; } h2 { font-size: 1.05rem; margin-top: 28px; }
  .card { background: #343746; border: 1px solid #44475a; border-radius: 12px; padding: 16px 20px; margin: 12px 0; }
  .ok { color: #50fa7b; } .bad { color: #ff5555; } .warn { color: #f1fa8c; } .muted { color: #a5a9c4; font-size: .9rem; }
  input[type=text] { width: 100%; box-sizing: border-box; padding: 10px; border-radius: 8px; border: 1px solid #6272a4; background: #282a36; color: #f8f8f2; font-family: monospace; }
  button { margin-top: 12px; padding: 10px 18px; border: 0; border-radius: 8px; background: #bd93f9; color: #fff; font-weight: 600; cursor: pointer; }
  .code { font: 700 1.8rem monospace; letter-spacing: .15em; background: #282a36; padding: 12px 16px; border-radius: 8px; display: inline-block; user-select: all; }
  table { width: 100%; border-collapse: collapse; font-size: .92rem; } td { padding: 4px 0; border-bottom: 1px solid #44475a; } td:last-child { text-align: right; font-family: monospace; }
  code { background: #282a36; padding: 1px 5px; border-radius: 4px; }
</style>
</head>
<body>
<main>
<h1>Conector LabNAS <span class="muted">v<?php echo lb_h(lb_version()); ?></span></h1>

<?php if ($linked): ?>
  <div class="card">
    <p class="ok">Este conector ya está enlazado con un LabNAS.</p>
    <p class="muted">Para enlazarlo con otro, borra <code>config.json</code> en la carpeta de datos y vuelve a abrir esta página.</p>
  </div>
<?php elseif ($code !== null): ?>
  <div class="card">
    <p>Código de enlace (vale <strong>30 minutos</strong> y una sola vez):</p>
    <p class="code"><?php echo lb_h(implode('-', str_split($code, 4))); ?></p>
    <p>En LabNAS ve a <strong>Configuración &gt; Acceso remoto</strong>, y pega:</p>
    <ul>
      <li>URL del puente: <code><?php echo lb_h($url); ?></code></li>
      <li>El código de arriba</li>
    </ul>
    <p class="muted">Este código no se vuelve a mostrar. Si lo pierdes, recarga esta página y genera otro.</p>
  </div>
<?php else: ?>
  <div class="card">
    <p>Paso 1: carpeta donde el conector guarda su clave y la cola de pedidos. Debe quedar <strong>fuera de public_html</strong>.</p>
    <?php if ($error !== null): ?><p class="bad"><?php echo lb_h($error); ?></p><?php endif; ?>
    <form method="post">
      <input type="text" name="data_dir" value="<?php echo lb_h($default); ?>">
      <button type="submit">Preparar y generar código</button>
    </form>
  </div>
<?php endif; ?>

<h2>Revisión del hosting</h2>
<div class="card">
  <table>
    <tr><td>PHP</td><td class="<?php echo $okPhp ? 'ok' : 'bad'; ?>"><?php echo lb_h(PHP_VERSION); ?></td></tr>
    <tr><td>Servidor</td><td><?php echo lb_h(isset($_SERVER['SERVER_SOFTWARE']) ? $_SERVER['SERVER_SOFTWARE'] : '?'); ?></td></tr>
    <tr><td>HTTPS</td><td class="<?php echo $https ? 'ok' : 'warn'; ?>"><?php echo $https ? 'sí' : 'no (actívalo en el panel: Let\'s Encrypt)'; ?></td></tr>
    <tr><td>Tamaño máximo de envío (post_max_size)</td><td><?php echo lb_h(lb_fmt_bytes($limits['post_max_size'])); ?></td></tr>
    <tr><td>Tamaño máximo de archivo (upload_max_filesize)</td><td><?php echo lb_h(lb_fmt_bytes($limits['upload_max_filesize'])); ?></td></tr>
    <tr><td>Tiempo máximo por pedido (max_execution_time)</td><td><?php echo $limits['max_execution_time'] > 0 ? (int) $limits['max_execution_time'] . ' s' : 'sin límite'; ?></td></tr>
    <tr><td>Memoria (memory_limit)</td><td><?php echo lb_h(lb_fmt_bytes($limits['memory_limit'])); ?></td></tr>
    <tr><td>Frontend incluido</td><td class="<?php echo is_dir(__DIR__ . '/web') ? 'ok' : 'warn'; ?>"><?php echo is_dir(__DIR__ . '/web') ? 'sí' : 'no (usa el paquete que entrega LabNAS)'; ?></td></tr>
  </table>
  <p class="muted">LabNAS lee estos límites al enlazarse y ajusta a ellos los tamaños y tiempos de espera.</p>
</div>
</main>
</body>
</html>
