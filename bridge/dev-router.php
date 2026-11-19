<?php
/**
 * Solo desarrollo y tests: `php -S 127.0.0.1:8080 dev-router.php`
 * (el servidor integrado de PHP no lee .htaccess).
 */
$path = parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH);
if ($path === '/instalar.php') {
    require __DIR__ . '/instalar.php';
    return true;
}
require __DIR__ . '/index.php';
return true;
