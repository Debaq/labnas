<?php
/**
 * LabNAS - conector puente. Cargador minimo: no se autoactualiza.
 * Compatible con PHP 7.0 en adelante.
 */
define('LB_ROOT', __DIR__);
require __DIR__ . '/app/bridge.php';
lb_main();
