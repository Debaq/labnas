use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;

pub type DbPool = Pool<SqliteConnectionManager>;

// ═══════════════════════════════════════
// Initialization
// ═══════════════════════════════════════

pub fn init_db() -> DbPool {
    let db_path = db_path();
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    println!("[LabNAS] Database: {}", db_path.display());

    // La clave de cifrado de secretos debe estar lista antes de abrir conexiones
    if let Some(dir) = db_path.parent() {
        crate::secrets::init(dir).expect("Error inicializando la clave de cifrado");
    }

    // foreign_keys, busy_timeout y las funciones de cifrado son por conexion:
    // aplicarlos a todas las del pool
    let manager = SqliteConnectionManager::file(&db_path).with_init(|c| {
        c.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;",
        )?;
        crate::secrets::register_sql_functions(c)
    });
    let pool = Pool::builder()
        .max_size(8)
        .build(manager)
        .expect("Error creando pool de base de datos");

    {
        let conn = pool.get().expect("Error obteniendo conexion de DB");
        create_schema(&conn);
        run_migrations(&conn);
        migrate_from_json(&conn);
        crate::roles::seed_levels(&conn);
    }

    // La DB guarda hashes y secretos: solo legible por el dueño
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Some(dir) = db_path.parent() {
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
        let _ = std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o600));
    }

    pool
}

/// Directorio de datos de LabNAS (DB, config). Nunca accesible desde el explorador.
pub fn data_dir() -> PathBuf {
    db_path().parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
}

/// Migraciones versionadas con PRAGMA user_version.
/// Agregar siempre al final; nunca modificar una ya publicada.
const MIGRATIONS: &[&str] = &[
    // 1: sesiones persistentes
    "CREATE TABLE IF NOT EXISTS sessions (
        token TEXT PRIMARY KEY,
        username TEXT NOT NULL REFERENCES web_users(username) ON DELETE CASCADE ON UPDATE CASCADE,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_sessions_username ON sessions(username);",
    // 2: auditoria persistente (antes 200 eventos en RAM)
    "CREATE TABLE IF NOT EXISTS audit_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        timestamp TEXT NOT NULL,
        username TEXT NOT NULL,
        action TEXT NOT NULL,
        details TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp);
    CREATE INDEX IF NOT EXISTS idx_audit_username ON audit_log(username);",
    // 3: tareas de respaldo programadas
    "CREATE TABLE IF NOT EXISTS backup_jobs (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        source TEXT NOT NULL,
        destination TEXT NOT NULL,
        hour INTEGER NOT NULL DEFAULT 3,
        minute INTEGER NOT NULL DEFAULT 0,
        keep INTEGER NOT NULL DEFAULT 7,
        include_db INTEGER NOT NULL DEFAULT 1,
        enabled INTEGER NOT NULL DEFAULT 1,
        last_run TEXT,
        last_status TEXT,
        last_message TEXT,
        created_at TEXT NOT NULL
    );",
    // 4: papelera
    "CREATE TABLE IF NOT EXISTS trash_items (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        original_path TEXT NOT NULL,
        trash_path TEXT NOT NULL,
        is_dir INTEGER NOT NULL,
        size INTEGER NOT NULL DEFAULT 0,
        deleted_by TEXT NOT NULL,
        deleted_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_trash_deleted_at ON trash_items(deleted_at);",
    // 5: cifrar secretos guardados en texto plano (labnas_encrypt no cifra dos veces)
    "UPDATE notification_config SET bot_token = labnas_encrypt(bot_token) WHERE bot_token IS NOT NULL AND bot_token != '';
    UPDATE printers3d SET api_key = labnas_encrypt(api_key) WHERE api_key IS NOT NULL AND api_key != '';
    UPDATE email_accounts SET password = labnas_encrypt(password) WHERE password != '';
    UPDATE settings SET value = labnas_encrypt(value) WHERE key IN ('groq_api_key', 'lastfm_api_key') AND value != '';",
    // 6: token por dispositivo de sensores (se guarda solo el hash)
    "ALTER TABLE sensor_devices ADD COLUMN token_hash TEXT;",
    // 7: permisos por carpeta (lectores/escritores de cada raiz de almacenamiento)
    "CREATE TABLE IF NOT EXISTS root_access (
        root TEXT PRIMARY KEY,
        readers TEXT NOT NULL DEFAULT '[\"*\"]',
        writers TEXT NOT NULL DEFAULT '[\"perm:write\"]'
    );",
    // 8: cola compartida de impresion 3D
    "CREATE TABLE IF NOT EXISTS print_queue (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        file_name TEXT NOT NULL DEFAULT '',
        printer_id TEXT REFERENCES printers3d(id) ON DELETE SET NULL,
        requested_by TEXT NOT NULL,
        notes TEXT NOT NULL DEFAULT '',
        grams REAL,
        seconds INTEGER,
        status TEXT NOT NULL DEFAULT 'pendiente',
        position INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_print_queue_status ON print_queue(status, position);",
    // 9: doble factor (TOTP). Secreto cifrado; codigos de recuperacion y la
    // contrasena de aplicacion (WebDAV) guardados como hash
    "ALTER TABLE web_users ADD COLUMN totp_secret TEXT;
    ALTER TABLE web_users ADD COLUMN totp_last_step INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE web_users ADD COLUMN app_password_hash TEXT;
    CREATE TABLE IF NOT EXISTS totp_recovery (
        username TEXT NOT NULL REFERENCES web_users(username) ON DELETE CASCADE ON UPDATE CASCADE,
        code_hash TEXT NOT NULL,
        PRIMARY KEY (username, code_hash)
    );",
    // 10: correo persistente (antes en RAM: se perdia y se re-clasificaba con IA en cada
    // revision), propuestas de la IA, y proveedor de IA generico en vez de la clave de Groq
    "CREATE TABLE IF NOT EXISTS emails (
        username TEXT NOT NULL REFERENCES web_users(username) ON DELETE CASCADE ON UPDATE CASCADE,
        uid INTEGER NOT NULL,
        message_id TEXT,
        from_addr TEXT NOT NULL DEFAULT '',
        subject TEXT NOT NULL DEFAULT '',
        date TEXT NOT NULL DEFAULT '',
        body_text TEXT NOT NULL DEFAULT '',
        ai_classification TEXT,
        ai_summary TEXT,
        ai_action TEXT,
        filter_label TEXT,
        filter_action TEXT,
        processed INTEGER NOT NULL DEFAULT 0,
        task_created INTEGER NOT NULL DEFAULT 0,
        fetched_at TEXT NOT NULL,
        PRIMARY KEY (username, uid)
    );
    CREATE INDEX IF NOT EXISTS idx_emails_user_fetched ON emails(username, fetched_at);
    CREATE TABLE IF NOT EXISTS ai_proposals (
        id TEXT PRIMARY KEY,
        username TEXT NOT NULL REFERENCES web_users(username) ON DELETE CASCADE ON UPDATE CASCADE,
        email_uid INTEGER,
        source TEXT NOT NULL DEFAULT 'ia',
        kind TEXT NOT NULL,
        title TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        date TEXT,
        time TEXT,
        end_time TEXT,
        location TEXT,
        status TEXT NOT NULL DEFAULT 'pendiente',
        created_ref TEXT,
        created_at TEXT NOT NULL,
        resolved_at TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_ai_proposals_user_status ON ai_proposals(username, status);
    INSERT OR IGNORE INTO settings (key, value)
        SELECT 'ai_api_key', value FROM settings WHERE key = 'groq_api_key' AND value != '';
    INSERT OR IGNORE INTO settings (key, value)
        SELECT 'ai_base_url', 'https://api.groq.com/openai/v1' FROM settings WHERE key = 'groq_api_key' AND value != '';
    INSERT OR IGNORE INTO settings (key, value)
        SELECT 'ai_model', 'llama-3.3-70b-versatile' FROM settings WHERE key = 'groq_api_key' AND value != '';
    DELETE FROM settings WHERE key = 'groq_api_key';",
    // 11: cuentas Microsoft con OAuth2 (password guarda el refresh token) y estado de la ultima revision
    "ALTER TABLE email_accounts ADD COLUMN auth TEXT NOT NULL DEFAULT 'password';
    ALTER TABLE email_accounts ADD COLUMN last_error TEXT;
    ALTER TABLE email_accounts ADD COLUMN last_check TEXT;",
    // 12: respaldos con restic (cifrados, a destinos remotos); secretos cifrados
    "ALTER TABLE backup_jobs ADD COLUMN kind TEXT NOT NULL DEFAULT 'rsync';
    ALTER TABLE backup_jobs ADD COLUMN restic_password TEXT;
    ALTER TABLE backup_jobs ADD COLUMN restic_env TEXT;",
    // 13: passkeys (WebAuthn): clave publica COSE ligada a un dominio (rp_id)
    "CREATE TABLE IF NOT EXISTS webauthn_credentials (
        id TEXT PRIMARY KEY,
        username TEXT NOT NULL REFERENCES web_users(username) ON DELETE CASCADE ON UPDATE CASCADE,
        name TEXT NOT NULL,
        rp_id TEXT NOT NULL,
        alg INTEGER NOT NULL,
        public_key BLOB NOT NULL,
        sign_count INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL,
        last_used TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_webauthn_user ON webauthn_credentials(username, rp_id);",
    // 14: token por usuario para la impresora de red (IPP): hash para buscarlo y
    // copia cifrada para mostrarle su URL
    "CREATE TABLE IF NOT EXISTS ipp_tokens (
        username TEXT PRIMARY KEY REFERENCES web_users(username) ON DELETE CASCADE ON UPDATE CASCADE,
        token_hash TEXT NOT NULL UNIQUE,
        token_enc TEXT NOT NULL,
        created_at TEXT NOT NULL
    );",
    // 15: camaras (USB del servidor e IP) y registro de fotos y grabaciones. La URL va
    // cifrada (puede llevar usuario y clave); los registros no se borran (trazabilidad)
    "CREATE TABLE IF NOT EXISTS cameras (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        kind TEXT NOT NULL,
        device TEXT NOT NULL DEFAULT '',
        url TEXT,
        resolution TEXT NOT NULL DEFAULT '',
        audio TEXT NOT NULL DEFAULT '',
        display_order INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS camera_recordings (
        id TEXT PRIMARY KEY,
        camera_id TEXT NOT NULL,
        camera_name TEXT NOT NULL,
        kind TEXT NOT NULL,
        username TEXT NOT NULL,
        path TEXT NOT NULL,
        started_at TEXT NOT NULL,
        ended_at TEXT,
        size INTEGER,
        status TEXT NOT NULL,
        deleted_by TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_camera_recordings_started ON camera_recordings(started_at);",
    // 16: bitacora inalterable (ver ledger.rs)
    MIGRATION_LEDGER,
    // 17: deteccion con IA en camaras (reglas y registro; el registro no se borra)
    "CREATE TABLE IF NOT EXISTS camera_detectors (
        id TEXT PRIMARY KEY,
        camera_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        classes TEXT NOT NULL DEFAULT '[]',
        min_score REAL NOT NULL,
        interval_s INTEGER NOT NULL,
        schedule TEXT,
        actions TEXT NOT NULL,
        cooldown_s INTEGER NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 1,
        created_by TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS camera_detections (
        id TEXT PRIMARY KEY,
        detector_id TEXT NOT NULL,
        camera_id TEXT NOT NULL,
        camera_name TEXT NOT NULL,
        kind TEXT NOT NULL,
        labels TEXT NOT NULL,
        score REAL NOT NULL,
        at TEXT NOT NULL,
        photo_id TEXT,
        clip_id TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_camera_detections_at ON camera_detections(at);",
    // 18: maquinas del laboratorio (catalogo con QR, reservas y fallas)
    "CREATE TABLE IF NOT EXISTS machines (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        kind TEXT NOT NULL DEFAULT 'otra',
        location TEXT NOT NULL DEFAULT '',
        description TEXT NOT NULL DEFAULT '',
        instructions TEXT NOT NULL DEFAULT '',
        responsible TEXT,
        status TEXT NOT NULL DEFAULT 'disponible',
        requires_approval INTEGER NOT NULL DEFAULT 0,
        max_hours REAL,
        inventory_item_id TEXT,
        printer3d_id TEXT,
        photo_id TEXT,
        created_by TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS machine_bookings (
        id TEXT PRIMARY KEY,
        machine_id TEXT NOT NULL,
        username TEXT NOT NULL,
        kind TEXT NOT NULL DEFAULT 'reserva',
        start TEXT NOT NULL,
        end TEXT NOT NULL,
        purpose TEXT NOT NULL DEFAULT '',
        status TEXT NOT NULL,
        decided_by TEXT,
        decided_at TEXT,
        reminded INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_machine_bookings_start ON machine_bookings(machine_id, start);
    CREATE TABLE IF NOT EXISTS machine_faults (
        id TEXT PRIMARY KEY,
        machine_id TEXT NOT NULL,
        reported_by TEXT NOT NULL,
        description TEXT NOT NULL,
        photo_id TEXT,
        created_at TEXT NOT NULL,
        resolved_by TEXT,
        resolved_at TEXT,
        resolution_note TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_machine_faults_machine ON machine_faults(machine_id, created_at);
    CREATE TABLE IF NOT EXISTS machine_photos (
        id TEXT PRIMARY KEY,
        machine_id TEXT NOT NULL,
        data BLOB NOT NULL,
        mime TEXT NOT NULL,
        created_by TEXT NOT NULL,
        created_at TEXT NOT NULL
    );",
    // 19: plazos con cuenta regresiva (contadores sueltos; los del Portafolio se calculan)
    // y registro de avisos enviados por umbral (30, 7, 1 y 0 dias)
    "CREATE TABLE IF NOT EXISTS deadlines (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        category TEXT NOT NULL DEFAULT 'otro',
        due_date TEXT NOT NULL,
        notes TEXT NOT NULL DEFAULT '',
        notify TEXT NOT NULL DEFAULT 'creador',
        portfolio_id TEXT,
        done_at TEXT,
        done_by TEXT,
        created_by TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_deadlines_due ON deadlines(done_at, due_date);
    CREATE TABLE IF NOT EXISTS deadline_alerts (
        key TEXT NOT NULL,
        due_date TEXT NOT NULL,
        threshold INTEGER NOT NULL,
        sent_at TEXT NOT NULL,
        PRIMARY KEY (key, due_date, threshold)
    );",
    // 20: induccion por maquina (idea 21). `status`: solicitada | vigente
    "ALTER TABLE machines ADD COLUMN requires_certification INTEGER NOT NULL DEFAULT 0;
    CREATE TABLE IF NOT EXISTS machine_certifications (
        machine_id TEXT NOT NULL,
        username TEXT NOT NULL,
        status TEXT NOT NULL,
        requested_at TEXT,
        granted_by TEXT,
        granted_at TEXT,
        expires_on TEXT,
        notes TEXT NOT NULL DEFAULT '',
        PRIMARY KEY (machine_id, username)
    );",
    // 21: detecciones de camaras: resultado del aviso por Telegram (tg_sent NULL = la regla
    // no avisa; tg_error = por que no llego) e indices para filtrar por regla y camara
    "ALTER TABLE camera_detections ADD COLUMN tg_sent INTEGER;
    ALTER TABLE camera_detections ADD COLUMN tg_error TEXT;
    CREATE INDEX IF NOT EXISTS idx_camera_detections_rule ON camera_detections(detector_id, at);
    CREATE INDEX IF NOT EXISTS idx_camera_detections_cam ON camera_detections(camera_id, at);",
    // 22: tokens de dispositivo (labnas-tray): solo el hash; valen hasta que se revocan
    "CREATE TABLE IF NOT EXISTS device_tokens (
        id TEXT PRIMARY KEY,
        username TEXT NOT NULL REFERENCES web_users(username) ON DELETE CASCADE ON UPDATE CASCADE,
        name TEXT NOT NULL,
        token_hash TEXT NOT NULL UNIQUE,
        created_at TEXT NOT NULL,
        last_used TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_device_tokens_user ON device_tokens(username);",
    // 23: piel de la interfaz por cuenta (la usa tambien labnas-tray)
    "ALTER TABLE web_users ADD COLUMN ui_theme TEXT;",
    // 24: roles editables con nivel por modulo (ver roles.rs). Los permisos sueltos de
    // antes (terminal, impresion, escribir archivos) pasan a excepciones por persona donde
    // difieren de la plantilla de su rol. `expires_at`: la cuenta deja de entrar (auditor).
    "CREATE TABLE IF NOT EXISTS roles (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        template TEXT,
        expiry_days INTEGER,
        display_order INTEGER NOT NULL DEFAULT 100
    );
    CREATE TABLE IF NOT EXISTS role_modules (
        role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
        module_id TEXT NOT NULL,
        level INTEGER NOT NULL CHECK (level BETWEEN 0 AND 3),
        PRIMARY KEY (role_id, module_id)
    );
    CREATE TABLE IF NOT EXISTS user_module_overrides (
        username TEXT NOT NULL REFERENCES web_users(username) ON DELETE CASCADE ON UPDATE CASCADE,
        module_id TEXT NOT NULL,
        level INTEGER NOT NULL CHECK (level BETWEEN 0 AND 3),
        PRIMARY KEY (username, module_id)
    );
    ALTER TABLE web_users ADD COLUMN expires_at INTEGER;
    INSERT OR IGNORE INTO roles (id, name, description, template, expiry_days, display_order) VALUES
        ('admin', 'Administrador', 'Acceso total, incluida la configuracion del sistema', 'admin', NULL, 0),
        ('operador', 'Operador', 'Personal del laboratorio: opera equipos y aprueba reservas', 'operador', NULL, 10),
        ('tecnico', 'Encargado tecnico', 'Mantiene maquinas, impresoras 3D, inventario y red', 'tecnico', NULL, 20),
        ('investigador', 'Investigador responsable', 'Dirige proyectos: gestiona tareas, portafolio y plazos', 'investigador', NULL, 30),
        ('administrativo', 'Administrativo', 'Compras, inventario y plazos; no opera equipos', 'administrativo', NULL, 40),
        ('docente', 'Docente', 'Gestiona tareas, portafolio y plazos de sus cursos', 'docente', NULL, 50),
        ('ayudante', 'Ayudante', 'Apoya a estudiantes: gestiona la cola de impresion 3D', 'ayudante', NULL, 60),
        ('estudiante', 'Estudiante', 'Reserva maquinas, imprime y lleva sus tareas', 'estudiante', NULL, 70),
        ('observador', 'Observador', 'Usuario general del laboratorio (rol de antes)', 'observador', NULL, 80),
        ('externo', 'Usuario externo', 'Solo reserva maquinas y encola piezas', 'externo', NULL, 90),
        ('auditor', 'Auditor', 'Ve todo sin modificar nada; su cuenta vence', 'auditor', 90, 100);
    INSERT OR IGNORE INTO user_module_overrides (username, module_id, level)
        SELECT username, 'terminal', 0 FROM web_users WHERE role = 'operador' AND perm_terminal = 0
        UNION ALL SELECT username, 'terminal', 2 FROM web_users WHERE role = 'observador' AND perm_terminal = 1
        UNION ALL SELECT username, 'printing', 1 FROM web_users WHERE role IN ('operador', 'observador') AND perm_impresion = 0
        UNION ALL SELECT username, 'files', 1 FROM web_users WHERE role = 'operador' AND perm_archivos_escritura = 0
        UNION ALL SELECT username, 'files', 2 FROM web_users WHERE role = 'observador' AND perm_archivos_escritura = 1;",
    // 25: niveles de acceso de cualquier registro o archivo (handlers::access): privado,
    // con autorizacion (personas con fecha de termino) o publico (enlace con token)
    "CREATE TABLE IF NOT EXISTS record_access (
        kind TEXT NOT NULL,
        record_id TEXT NOT NULL,
        visibility TEXT NOT NULL DEFAULT 'privado' CHECK (visibility IN ('privado', 'autorizado', 'publico')),
        public_token TEXT UNIQUE,
        updated_by TEXT NOT NULL,
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (kind, record_id)
    );
    CREATE TABLE IF NOT EXISTS record_grants (
        id TEXT PRIMARY KEY,
        kind TEXT NOT NULL,
        record_id TEXT NOT NULL,
        username TEXT NOT NULL REFERENCES web_users(username) ON DELETE CASCADE ON UPDATE CASCADE,
        expires_at INTEGER NOT NULL,
        note TEXT NOT NULL DEFAULT '',
        granted_by TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_record_grants_user ON record_grants(username, expires_at);
    CREATE INDEX IF NOT EXISTS idx_record_grants_record ON record_grants(kind, record_id);",
    // 26: mantenimiento y calibracion por maquina (idea 5): plan de tareas por dias y/o
    // horas de uso, bitacora que no se edita y ajuste de horas por horometro
    "ALTER TABLE machines ADD COLUMN usage_adjust_hours REAL NOT NULL DEFAULT 0;
    CREATE TABLE IF NOT EXISTS machine_maintenance_tasks (
        id TEXT PRIMARY KEY,
        machine_id TEXT NOT NULL,
        title TEXT NOT NULL,
        kind TEXT NOT NULL DEFAULT 'mantencion',
        every_days INTEGER,
        every_hours REAL,
        instructions TEXT NOT NULL DEFAULT '',
        blocks INTEGER NOT NULL DEFAULT 0,
        last_done_at TEXT,
        last_done_hours REAL,
        hours_alerted REAL,
        created_by TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_maint_tasks_machine ON machine_maintenance_tasks(machine_id);
    CREATE TABLE IF NOT EXISTS machine_maintenance_log (
        id TEXT PRIMARY KEY,
        machine_id TEXT NOT NULL,
        task_id TEXT,
        kind TEXT NOT NULL,
        title TEXT NOT NULL,
        note TEXT NOT NULL DEFAULT '',
        usage_hours REAL NOT NULL DEFAULT 0,
        photo_id TEXT,
        done_by TEXT NOT NULL,
        done_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_maint_log_machine ON machine_maintenance_log(machine_id, done_at);",
    // 27: pantallas del laboratorio (ideas 8 y 47): enlace secreto de solo lectura por equipo
    "CREATE TABLE IF NOT EXISTS display_screens (
        token TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        sections TEXT NOT NULL DEFAULT '[]',
        created_by TEXT NOT NULL,
        created_at TEXT NOT NULL,
        last_seen_at TEXT
    );",
    // 28: alarmas criticas con escalamiento (idea 6): alerta critica con lista de personas
    // en orden e incidentes con quien fue avisado, quien confirmo y cuando volvio a rango
    "ALTER TABLE sensor_alerts ADD COLUMN critical INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE sensor_alerts ADD COLUMN escalation TEXT NOT NULL DEFAULT '[]';
    ALTER TABLE sensor_alerts ADD COLUMN escalate_minutes INTEGER NOT NULL DEFAULT 10;
    CREATE TABLE IF NOT EXISTS sensor_incidents (
        id TEXT PRIMARY KEY,
        alert_id TEXT NOT NULL,
        device_id TEXT NOT NULL,
        device_name TEXT NOT NULL,
        key TEXT NOT NULL,
        condition TEXT NOT NULL,
        threshold REAL NOT NULL,
        first_value REAL NOT NULL,
        worst_value REAL NOT NULL,
        started_at TEXT NOT NULL,
        step INTEGER NOT NULL DEFAULT 0,
        last_notified_at TEXT NOT NULL,
        notified TEXT NOT NULL DEFAULT '[]',
        acked_by TEXT,
        acked_at TEXT,
        resolved_at TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_sensor_incidents_open ON sensor_incidents(resolved_at, alert_id);",
    // 29: inventario ampliado (ideas 1, 31, 33, 34): stock minimo con aviso, vencimiento,
    // items prestables y prestamos con fecha de devolucion
    "ALTER TABLE inventory_items ADD COLUMN min_quantity REAL;
    ALTER TABLE inventory_items ADD COLUMN expires_on TEXT;
    ALTER TABLE inventory_items ADD COLUMN loanable INTEGER NOT NULL DEFAULT 0;
    CREATE TABLE IF NOT EXISTS inventory_stock_alerts (
        item_id TEXT PRIMARY KEY,
        alerted_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS inventory_loans (
        id TEXT PRIMARY KEY,
        item_id TEXT NOT NULL,
        borrower TEXT NOT NULL,
        quantity REAL NOT NULL DEFAULT 1,
        lent_by TEXT NOT NULL,
        lent_at TEXT NOT NULL,
        due_on TEXT NOT NULL,
        note TEXT NOT NULL DEFAULT '',
        returned_at TEXT,
        returned_to TEXT,
        reminded_on TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_inventory_loans_open ON inventory_loans(returned_at, due_on);
    CREATE INDEX IF NOT EXISTS idx_inventory_loans_item ON inventory_loans(item_id);",
    // 30: sesiones de uso real de maquinas (empezar/terminar desde la ficha)
    "CREATE TABLE IF NOT EXISTS machine_usage (
        id TEXT PRIMARY KEY,
        machine_id TEXT NOT NULL,
        username TEXT NOT NULL,
        booking_id TEXT,
        project TEXT NOT NULL DEFAULT '',
        started_at TEXT NOT NULL,
        ended_at TEXT,
        note TEXT NOT NULL DEFAULT ''
    );
    CREATE UNIQUE INDEX IF NOT EXISTS idx_machine_usage_open ON machine_usage(machine_id) WHERE ended_at IS NULL;
    CREATE INDEX IF NOT EXISTS idx_machine_usage_started ON machine_usage(started_at);
    CREATE INDEX IF NOT EXISTS idx_machine_usage_booking ON machine_usage(booking_id);",
    // 31: talleres con inscripcion, cupos, asistencia y certificados (idea 40)
    "CREATE TABLE IF NOT EXISTS workshop_settings (
        entry_id TEXT PRIMARY KEY,
        capacity INTEGER NOT NULL DEFAULT 0,
        registration_open INTEGER NOT NULL DEFAULT 0,
        public_token TEXT UNIQUE,
        sessions TEXT NOT NULL DEFAULT '[]',
        min_attendance INTEGER NOT NULL DEFAULT 75
    );
    CREATE TABLE IF NOT EXISTS workshop_registrations (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        entry_id TEXT NOT NULL,
        name TEXT NOT NULL,
        email TEXT NOT NULL DEFAULT '',
        username TEXT,
        affiliation TEXT NOT NULL DEFAULT '',
        registered_at TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'inscrito',
        attended TEXT NOT NULL DEFAULT '[]',
        certificate_code TEXT UNIQUE,
        certificate_at TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_workshop_regs_entry ON workshop_registrations(entry_id, status);",
    // 32: seguridad laboral (idea 17): capacitaciones generales, fichas de seguridad e incidentes
    "CREATE TABLE IF NOT EXISTS safety_trainings (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        valid_months INTEGER,
        mandatory INTEGER NOT NULL DEFAULT 0,
        created_by TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS safety_training_records (
        training_id TEXT NOT NULL,
        username TEXT NOT NULL,
        granted_by TEXT NOT NULL,
        granted_on TEXT NOT NULL,
        expires_on TEXT,
        notes TEXT NOT NULL DEFAULT '',
        PRIMARY KEY (training_id, username)
    );
    CREATE TABLE IF NOT EXISTS safety_sheets (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        kind TEXT NOT NULL DEFAULT 'material',
        content TEXT NOT NULL DEFAULT '',
        url TEXT NOT NULL DEFAULT '',
        machine_id TEXT,
        inventory_item_id TEXT,
        created_by TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS safety_sheet_files (
        sheet_id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        mime TEXT NOT NULL,
        data BLOB NOT NULL
    );
    CREATE TABLE IF NOT EXISTS safety_incidents (
        id TEXT PRIMARY KEY,
        reported_by TEXT NOT NULL,
        reported_at TEXT NOT NULL,
        occurred_at TEXT NOT NULL,
        place TEXT NOT NULL DEFAULT '',
        machine_id TEXT,
        severity TEXT NOT NULL,
        description TEXT NOT NULL,
        actions TEXT NOT NULL DEFAULT '',
        status TEXT NOT NULL DEFAULT 'abierto',
        closed_by TEXT,
        closed_at TEXT,
        closing_note TEXT NOT NULL DEFAULT ''
    );
    CREATE INDEX IF NOT EXISTS idx_safety_incidents_status ON safety_incidents(status, occurred_at);
    CREATE TABLE IF NOT EXISTS safety_incident_photos (
        incident_id TEXT PRIMARY KEY,
        mime TEXT NOT NULL,
        data BLOB NOT NULL
    );",
    // 33: registro de entrada al laboratorio (idea 48): personas con cuenta o visitas,
    // carrera o afiliacion, entrada y salida
    "CREATE TABLE IF NOT EXISTS safety_presence (
        id TEXT PRIMARY KEY,
        username TEXT,
        visitor_name TEXT,
        affiliation TEXT NOT NULL DEFAULT '',
        registered_by TEXT NOT NULL,
        entered_at TEXT NOT NULL,
        left_at TEXT,
        auto_closed INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX IF NOT EXISTS idx_safety_presence_open ON safety_presence(left_at, username);
    CREATE INDEX IF NOT EXISTS idx_safety_presence_day ON safety_presence(entered_at);",
    // 34: parametros de corte colaborativos (idea 29) y votos «me funciono»
    "CREATE TABLE IF NOT EXISTS cut_params (
        id TEXT PRIMARY KEY,
        machine_id TEXT NOT NULL,
        material TEXT NOT NULL,
        thickness_mm REAL,
        operation TEXT NOT NULL DEFAULT 'corte',
        power REAL,
        speed REAL,
        passes INTEGER,
        notes TEXT NOT NULL DEFAULT '',
        result TEXT NOT NULL DEFAULT 'bueno',
        photo_id TEXT,
        created_by TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_cut_params_machine ON cut_params(machine_id, material);
    CREATE TABLE IF NOT EXISTS cut_param_votes (
        param_id TEXT NOT NULL,
        username TEXT NOT NULL,
        PRIMARY KEY (param_id, username)
    );",
    // 35: retazos de plancha para reutilizar (idea 32)
    "CREATE TABLE IF NOT EXISTS offcuts (
        id TEXT PRIMARY KEY,
        material TEXT NOT NULL,
        thickness_mm REAL,
        width_mm REAL NOT NULL,
        height_mm REAL NOT NULL,
        location TEXT NOT NULL DEFAULT '',
        notes TEXT NOT NULL DEFAULT '',
        photo BLOB,
        photo_mime TEXT,
        added_by TEXT NOT NULL,
        added_at TEXT NOT NULL,
        taken_by TEXT,
        taken_at TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_offcuts_open ON offcuts(taken_at, material);",
    // 36: reciclaje: kilos triturados, filamento recuperado y desecho por material (idea 36)
    "CREATE TABLE IF NOT EXISTS recycling_log (
        id TEXT PRIMARY KEY,
        date TEXT NOT NULL,
        material TEXT NOT NULL,
        kind TEXT NOT NULL,
        kg REAL NOT NULL,
        notes TEXT NOT NULL DEFAULT '',
        created_by TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_recycling_date ON recycling_log(date);",
    // 37: base del modulo Finanzas (etapa 1, ver docs/FINANZAS.md). Esquema comun a
    // presupuesto, compras, rendicion y transparencia; cada parte agrega sus tablas propias
    // en migraciones siguientes. Un proyecto es una entrada del Portafolio.
    "CREATE TABLE IF NOT EXISTS fin_projects (
        project_id TEXT PRIMARY KEY,
        currency TEXT NOT NULL DEFAULT 'CLP',
        responsible TEXT NOT NULL DEFAULT '',
        alert_pct REAL NOT NULL DEFAULT 80,
        closed_at TEXT,
        created_by TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS fin_budget_items (
        id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL,
        category TEXT NOT NULL,
        name TEXT NOT NULL,
        approved REAL NOT NULL DEFAULT 0,
        notes TEXT NOT NULL DEFAULT '',
        created_by TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_fin_budget_project ON fin_budget_items(project_id);
    CREATE TABLE IF NOT EXISTS fin_commitments (
        source TEXT NOT NULL,
        source_id TEXT NOT NULL,
        project_id TEXT NOT NULL,
        budget_item_id TEXT,
        label TEXT NOT NULL DEFAULT '',
        committed REAL NOT NULL DEFAULT 0,
        spent REAL NOT NULL DEFAULT 0,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (source, source_id)
    );
    CREATE INDEX IF NOT EXISTS idx_fin_commit_item ON fin_commitments(project_id, budget_item_id);
    CREATE TABLE IF NOT EXISTS fin_files (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        mime TEXT NOT NULL,
        size INTEGER NOT NULL,
        sha256 TEXT NOT NULL,
        data BLOB NOT NULL,
        uploaded_by TEXT NOT NULL,
        uploaded_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS fin_purchases (
        id TEXT PRIMARY KEY,
        folio INTEGER NOT NULL UNIQUE,
        project_id TEXT NOT NULL,
        budget_item_id TEXT,
        title TEXT NOT NULL,
        justification TEXT NOT NULL DEFAULT '',
        estimated_amount REAL NOT NULL DEFAULT 0,
        final_amount REAL,
        status TEXT NOT NULL DEFAULT 'solicitud',
        requested_by TEXT NOT NULL,
        approved_by TEXT,
        received_by TEXT,
        chosen_quote_id TEXT,
        choice_reason TEXT NOT NULL DEFAULT '',
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_fin_purchases_project ON fin_purchases(project_id, status);
    CREATE TABLE IF NOT EXISTS fin_purchase_steps (
        id TEXT PRIMARY KEY,
        purchase_id TEXT NOT NULL,
        step TEXT NOT NULL,
        username TEXT NOT NULL,
        at TEXT NOT NULL,
        note TEXT NOT NULL DEFAULT '',
        passkey INTEGER NOT NULL DEFAULT 0,
        ledger_seq INTEGER
    );
    CREATE INDEX IF NOT EXISTS idx_fin_steps_purchase ON fin_purchase_steps(purchase_id, at);
    CREATE TABLE IF NOT EXISTS fin_quotes (
        id TEXT PRIMARY KEY,
        purchase_id TEXT NOT NULL,
        supplier TEXT NOT NULL,
        supplier_rut TEXT NOT NULL DEFAULT '',
        amount REAL NOT NULL,
        valid_until TEXT,
        file_id TEXT,
        notes TEXT NOT NULL DEFAULT '',
        created_by TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS fin_documents (
        id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL,
        purchase_id TEXT,
        kind TEXT NOT NULL,
        folio TEXT NOT NULL DEFAULT '',
        issuer_rut TEXT NOT NULL DEFAULT '',
        issuer_name TEXT NOT NULL DEFAULT '',
        issued_on TEXT,
        net_amount REAL,
        tax_amount REAL,
        total_amount REAL,
        file_id TEXT,
        extracted_by TEXT NOT NULL DEFAULT 'manual',
        confirmed_by TEXT,
        created_by TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_fin_docs_project ON fin_documents(project_id, kind);
    CREATE TABLE IF NOT EXISTS fin_receptions (
        id TEXT PRIMARY KEY,
        purchase_id TEXT NOT NULL,
        received_by TEXT NOT NULL,
        received_at TEXT NOT NULL,
        conforming INTEGER NOT NULL DEFAULT 1,
        notes TEXT NOT NULL DEFAULT '',
        photo_file_id TEXT
    );
    ALTER TABLE inventory_items ADD COLUMN project_id TEXT;
    ALTER TABLE inventory_items ADD COLUMN inventory_number TEXT NOT NULL DEFAULT '';
    ALTER TABLE inventory_items ADD COLUMN responsible TEXT NOT NULL DEFAULT '';",
    // 38: (reservada) presupuesto, ejecucion, caja chica e ingresos — agente A
    // 39: (reservada) flujo de compras — agente B
    // 40: (reservada) rendicion, expediente, manual y anomalias — agente C
    // 41: (reservada) transparencia, solicitudes de informacion y viajes — agente D
];

/// Migracion 16. Publicada: no modificar (ver `MIGRATIONS`).
/// `prev_hash` UNIQUE impide bifurcar la cadena; los triggers, editar o borrar.
pub(crate) const MIGRATION_LEDGER: &str = "CREATE TABLE IF NOT EXISTS ledger_entries (
        seq INTEGER PRIMARY KEY,
        timestamp TEXT NOT NULL,
        username TEXT NOT NULL,
        kind TEXT NOT NULL,
        subject TEXT NOT NULL DEFAULT '',
        annuls INTEGER,
        data TEXT NOT NULL,
        prev_hash TEXT NOT NULL UNIQUE,
        hash TEXT NOT NULL UNIQUE
    );
    CREATE UNIQUE INDEX IF NOT EXISTS idx_ledger_annuls ON ledger_entries(annuls) WHERE annuls IS NOT NULL;
    CREATE INDEX IF NOT EXISTS idx_ledger_kind ON ledger_entries(kind);
    CREATE TRIGGER IF NOT EXISTS ledger_no_update BEFORE UPDATE ON ledger_entries
    BEGIN SELECT RAISE(ABORT, 'bitacora inalterable: no se puede editar'); END;
    CREATE TRIGGER IF NOT EXISTS ledger_no_delete BEFORE DELETE ON ledger_entries
    BEGIN SELECT RAISE(ABORT, 'bitacora inalterable: no se puede borrar'); END;";

fn run_migrations(conn: &Connection) {
    let current: usize = conn
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .unwrap_or(0) as usize;

    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let version = i + 1;
        let tx = conn.unchecked_transaction().expect("Error iniciando migracion");
        tx.execute_batch(sql)
            .unwrap_or_else(|e| panic!("Error en migracion {}: {}", version, e));
        tx.pragma_update(None, "user_version", version as i64)
            .expect("Error actualizando user_version");
        tx.commit().expect("Error confirmando migracion");
        println!("[LabNAS] Migracion {} aplicada", version);
    }
}

fn db_path() -> PathBuf {
    #[cfg(test)]
    if let Some(h) = crate::config::TEST_HOME.read().ok().and_then(|h| h.clone()) {
        return PathBuf::from(h).join(".labnas").join("labnas.db");
    }
    if let Ok(p) = std::env::var("LABNAS_DB") {
        return PathBuf::from(p);
    }
    PathBuf::from(crate::config::resolve_home())
        .join(".labnas")
        .join("labnas.db")
}

fn create_schema(conn: &Connection) {
    conn.execute_batch(SCHEMA).expect("Error creando schema SQLite");
}

// ═══════════════════════════════════════
// JSON Migration
// ═══════════════════════════════════════

fn migrate_from_json(conn: &Connection) {
    let json_path = crate::config::config_path();
    // Always seed default modules
    seed_modules(conn);

    if !json_path.exists() {
        println!("[LabNAS] No config.json encontrado, instalacion nueva");
        // Insert singleton rows
        conn.execute(
            "INSERT OR IGNORE INTO branding (id) VALUES (1)",
            [],
        ).ok();
        conn.execute(
            "INSERT OR IGNORE INTO notification_config (id) VALUES (1)",
            [],
        ).ok();
        return;
    }

    // Check if DB already has data (migration already done)
    let user_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM web_users", [], |r| r.get(0))
        .unwrap_or(0);
    let printer_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM printers3d", [], |r| r.get(0))
        .unwrap_or(0);
    let settings_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM settings", [], |r| r.get(0))
        .unwrap_or(0);

    if user_count > 0 || printer_count > 0 || settings_count > 0 {
        println!("[LabNAS] DB ya tiene datos, saltando migracion JSON");
        return;
    }

    println!("[LabNAS] Migrando config.json -> SQLite...");

    let json_str = match std::fs::read_to_string(&json_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[LabNAS] Error leyendo config.json: {}", e);
            return;
        }
    };

    let config: crate::config::LabNasConfig = match serde_json::from_str(&json_str) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[LabNAS] Error parseando config.json: {}", e);
            return;
        }
    };

    // Begin transaction for atomic migration
    let tx = match conn.unchecked_transaction() {
        Ok(tx) => tx,
        Err(e) => {
            eprintln!("[LabNAS] Error iniciando transaccion: {}", e);
            return;
        }
    };

    if let Err(e) = do_migration(&tx, &config) {
        eprintln!("[LabNAS] Error en migracion, config.json NO modificado: {}", e);
        tx.rollback().ok();
        return;
    }

    if let Err(e) = tx.commit() {
        eprintln!("[LabNAS] Error en commit de migracion: {}", e);
        return;
    }

    // Rename config.json to config.json.bak
    let bak_path = json_path.with_extension("json.bak");
    match std::fs::rename(&json_path, &bak_path) {
        Ok(_) => println!(
            "[LabNAS] Migracion exitosa! config.json -> config.json.bak"
        ),
        Err(e) => eprintln!(
            "[LabNAS] Migracion exitosa pero no se pudo renombrar config.json: {}",
            e
        ),
    }
}

fn do_migration(conn: &Connection, config: &crate::config::LabNasConfig) -> Result<(), String> {
    // Settings
    let settings: Vec<(&str, String)> = vec![
        ("mdns_enabled", config.mdns_enabled.to_string()),
        ("mdns_hostname", config.mdns_hostname.clone()),
        ("upload_limit_mb", config.upload_limit_mb.to_string()),
        (
            "mpv_extra_args",
            serde_json::to_string(&config.mpv_extra_args).unwrap_or_default(),
        ),
    ];
    if let Some(ref key) = config.lastfm_api_key {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)",
            params!["lastfm_api_key", key],
        )
        .map_err(|e| format!("settings lastfm: {}", e))?;
    }
    for (k, v) in &settings {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)",
            params![k, v],
        )
        .map_err(|e| format!("settings {}: {}", k, e))?;
    }

    // Branding
    let b = &config.branding;
    conn.execute(
        "INSERT OR REPLACE INTO branding (id, lab_name, institution, logo_url, mission, vision, website, contact_email, location, accent_color)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![b.lab_name, b.institution, b.logo_url, b.mission, b.vision, b.website, b.contact_email, b.location, b.accent_color],
    ).map_err(|e| format!("branding: {}", e))?;

    // Web users
    for u in &config.web_users {
        let role_str = serde_json::to_value(&u.role)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "pendiente".to_string());
        conn.execute(
            "INSERT INTO web_users (username, password_hash, role, perm_terminal, perm_impresion, perm_archivos_escritura, linked_telegram)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                u.username, u.password_hash, role_str,
                u.permissions.terminal, u.permissions.impresion, u.permissions.archivos_escritura,
                u.linked_telegram,
            ],
        ).map_err(|e| format!("web_users: {}", e))?;
    }

    // Notification config
    let nc = &config.notifications;
    conn.execute(
        "INSERT OR REPLACE INTO notification_config (id, bot_token, bot_username, daily_enabled, daily_hour, daily_minute)
         VALUES (1, labnas_encrypt(?1), ?2, ?3, ?4, ?5)",
        params![nc.bot_token, nc.bot_username, nc.daily_enabled, nc.daily_hour, nc.daily_minute],
    ).map_err(|e| format!("notification_config: {}", e))?;

    // Telegram chats
    for chat in &nc.telegram_chats {
        let role_str = serde_json::to_value(&chat.role)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "pendiente".to_string());
        conn.execute(
            "INSERT INTO telegram_chats (chat_id, name, username, role, perm_terminal, perm_impresion, perm_archivos_escritura, linked_web_user, daily_enabled, daily_hour, daily_minute)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                chat.chat_id, chat.name, chat.username, role_str,
                chat.permissions.terminal, chat.permissions.impresion, chat.permissions.archivos_escritura,
                chat.linked_web_user, chat.daily_enabled, chat.daily_hour, chat.daily_minute,
            ],
        ).map_err(|e| format!("telegram_chats: {}", e))?;
    }

    // Known devices
    for d in &config.known_devices {
        conn.execute(
            "INSERT INTO known_devices (mac, label, icon) VALUES (?1, ?2, ?3)",
            params![d.mac, d.label, d.icon],
        )
        .map_err(|e| format!("known_devices: {}", e))?;
    }

    // Services
    for s in &config.services {
        conn.execute(
            "INSERT INTO services (port, name, description, icon) VALUES (?1, ?2, ?3, ?4)",
            params![s.port, s.name, s.description, s.icon],
        )
        .map_err(|e| format!("services: {}", e))?;
    }

    // Printer 3D sections
    for s in &config.printer3d_sections {
        conn.execute(
            "INSERT INTO printer3d_sections (id, name, \"order\") VALUES (?1, ?2, ?3)",
            params![s.id, s.name, s.order],
        )
        .map_err(|e| format!("printer3d_sections: {}", e))?;
    }

    // Printers 3D
    for p in &config.printers3d {
        let pt = serde_json::to_value(&p.printer_type)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "Moonraker".to_string());
        conn.execute(
            "INSERT INTO printers3d (id, name, ip, port, printer_type, api_key, camera_url, power_watts, electricity_cost_kwh, section_id, \"order\")
             VALUES (?1, ?2, ?3, ?4, ?5, labnas_encrypt(?6), ?7, ?8, ?9, ?10, ?11)",
            params![
                p.id, p.name, p.ip, p.port, pt, p.api_key, p.camera_url,
                p.power_watts, p.electricity_cost_kwh, p.section_id, p.order,
            ],
        ).map_err(|e| format!("printers3d: {}", e))?;
    }

    // CUPS printers
    for cp in &config.cups_printers {
        conn.execute(
            "INSERT INTO cups_printers (name, ink_per_page, paper_carta, paper_oficio, paper_special, total_jobs, total_pages, pages_carta, pages_oficio, pages_special)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                cp.name, cp.costs.ink_per_page, cp.costs.paper_carta, cp.costs.paper_oficio, cp.costs.paper_special,
                cp.stats.total_jobs, cp.stats.total_pages, cp.stats.pages_carta, cp.stats.pages_oficio, cp.stats.pages_special,
            ],
        ).map_err(|e| format!("cups_printers: {}", e))?;

        for (username, stats) in &cp.user_stats {
            conn.execute(
                "INSERT INTO cups_printer_user_stats (printer_name, username, total_jobs, total_pages, pages_carta, pages_oficio, pages_special)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![cp.name, username, stats.total_jobs, stats.total_pages, stats.pages_carta, stats.pages_oficio, stats.pages_special],
            ).map_err(|e| format!("cups_user_stats: {}", e))?;
        }
    }

    // Notes
    for note in &config.notes {
        conn.execute(
            "INSERT INTO notes (id, title, content, created_by, updated_by, shared_with, is_public, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                note.id, note.title, note.content, note.created_by, note.updated_by,
                serde_json::to_string(&note.shared_with).unwrap_or_default(),
                note.is_public,
                note.created_at.to_rfc3339(), note.updated_at.to_rfc3339(),
            ],
        ).map_err(|e| format!("notes: {}", e))?;
    }

    // Email
    if let Some(ref key) = config.email.groq_api_key {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES
                ('ai_api_key', labnas_encrypt(?1)),
                ('ai_base_url', 'https://api.groq.com/openai/v1'),
                ('ai_model', 'llama-3.3-70b-versatile')",
            params![key],
        )
        .map_err(|e| format!("groq_key: {}", e))?;
    }
    for acc in &config.email.accounts {
        let proto = serde_json::to_value(&acc.protocol)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "imap".to_string());
        conn.execute(
            "INSERT INTO email_accounts (username, host, port, protocol, email, password, filters)
             VALUES (?1, ?2, ?3, ?4, ?5, labnas_encrypt(?6), ?7)",
            params![
                acc.username, acc.host, acc.port, proto, acc.email, acc.password,
                serde_json::to_string(&acc.filters).unwrap_or_default(),
            ],
        ).map_err(|e| format!("email_accounts: {}", e))?;
    }

    // Tasks - Projects
    for p in &config.tasks.projects {
        conn.execute(
            "INSERT INTO projects (id, name, description, created_by, members, member_tags, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                p.id, p.name, p.description, p.created_by,
                serde_json::to_string(&p.members).unwrap_or_default(),
                serde_json::to_string(&p.member_tags).unwrap_or_default(),
                p.created_at.to_rfc3339(),
            ],
        ).map_err(|e| format!("projects: {}", e))?;
    }

    // Tasks
    for t in &config.tasks.tasks {
        let status_str = serde_json::to_value(&t.status)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "pendiente".to_string());
        conn.execute(
            "INSERT INTO tasks (id, project_id, title, description, assigned_to, status, created_by, due_date, due_time, requires_confirmation, insistent, reminder_minutes, confirmed_by, rejected_by, created_at, last_reminder)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                t.id, t.project_id, t.title, t.description,
                serde_json::to_string(&t.assigned_to).unwrap_or_default(),
                status_str, t.created_by, t.due_date, t.due_time,
                t.requires_confirmation, t.insistent, t.reminder_minutes,
                serde_json::to_string(&t.confirmed_by).unwrap_or_default(),
                serde_json::to_string(&t.rejected_by).unwrap_or_default(),
                t.created_at.to_rfc3339(),
                t.last_reminder.map(|d| d.to_rfc3339()),
            ],
        ).map_err(|e| format!("tasks: {}", e))?;
    }

    // Calendar events
    for ev in &config.tasks.events {
        conn.execute(
            "INSERT INTO calendar_events (id, title, description, date, time, end_time, location, created_by, invitees, accepted, declined, remind_before_min, reminded, notify_telegram, recurrence, recurrence_end, created_at, category)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                ev.id, ev.title, ev.description, ev.date, ev.time, ev.end_time, ev.location,
                ev.created_by,
                serde_json::to_string(&ev.invitees).unwrap_or_default(),
                serde_json::to_string(&ev.accepted).unwrap_or_default(),
                serde_json::to_string(&ev.declined).unwrap_or_default(),
                ev.remind_before_min, ev.reminded, ev.notify_telegram,
                ev.recurrence, ev.recurrence_end, ev.created_at.to_rfc3339(), ev.category,
            ],
        ).map_err(|e| format!("calendar_events: {}", e))?;
    }

    // Event categories
    for cat in &config.tasks.event_categories {
        conn.execute(
            "INSERT INTO event_categories (id, name, color) VALUES (?1, ?2, ?3)",
            params![cat.id, cat.name, cat.color],
        )
        .map_err(|e| format!("event_categories: {}", e))?;
    }

    // Inventory categories
    for cat in &config.inventory.categories {
        conn.execute(
            "INSERT INTO inventory_categories (id, name, icon, description) VALUES (?1, ?2, ?3, ?4)",
            params![cat.id, cat.name, cat.icon, cat.description],
        )
        .map_err(|e| format!("inventory_categories: {}", e))?;
    }

    // Inventory items
    for item in &config.inventory.items {
        let status_str = serde_json::to_value(&item.status)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "activo".to_string());
        conn.execute(
            "INSERT INTO inventory_items (id, category_id, name, description, quantity, unit, cost, supplier, supplier_url, location, serial_number, purchase_date, status, notes, brand, model_name, material_type, filament_diameter, filament_weight, filament_remaining, filament_color, filament_density, custom_fields, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
            params![
                item.id, item.category_id, item.name, item.description,
                item.quantity, item.unit, item.cost, item.supplier, item.supplier_url,
                item.location, item.serial_number, item.purchase_date,
                status_str, item.notes, item.brand, item.model_name,
                item.material_type, item.filament_diameter, item.filament_weight,
                item.filament_remaining, item.filament_color, item.filament_density,
                serde_json::to_string(&item.custom_fields).unwrap_or_default(),
                item.created_at.to_rfc3339(),
            ],
        ).map_err(|e| format!("inventory_items: {}", e))?;
    }

    // Print history
    for ph in &config.inventory.print_history {
        conn.execute(
            "INSERT INTO print_history (id, printer_id, file_name, filament_id, weight_grams, print_time_minutes, material_cost, electricity_cost, total_cost, success, notes, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                ph.id, ph.printer_id, ph.file_name, ph.filament_id,
                ph.weight_grams, ph.print_time_minutes, ph.material_cost,
                ph.electricity_cost, ph.total_cost, ph.success, ph.notes,
                ph.created_at.to_rfc3339(),
            ],
        ).map_err(|e| format!("print_history: {}", e))?;
    }

    // Portfolio
    for entry in &config.portfolio.entries {
        let type_str = serde_json::to_value(&entry.entry_type)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "project".to_string());
        let scope_str = serde_json::to_value(&entry.scope)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "own".to_string());
        let status_str = serde_json::to_value(&entry.status)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "planned".to_string());
        conn.execute(
            "INSERT INTO portfolio_entries (id, entry_type, scope, title, description, institution, url, contact, funding_source, budget, principal_investigator, collaborators, participants, start_date, end_date, hours, modality, status, requirements, milestones, tags, notes, related_files, related_inventory, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
            params![
                entry.id, type_str, scope_str, entry.title, entry.description,
                entry.institution, entry.url, entry.contact, entry.funding_source, entry.budget,
                entry.principal_investigator,
                serde_json::to_string(&entry.collaborators).unwrap_or_default(),
                serde_json::to_string(&entry.participants).unwrap_or_default(),
                entry.start_date, entry.end_date, entry.hours, entry.modality, status_str,
                serde_json::to_string(&entry.requirements).unwrap_or_default(),
                serde_json::to_string(&entry.milestones).unwrap_or_default(),
                serde_json::to_string(&entry.tags).unwrap_or_default(),
                entry.notes,
                serde_json::to_string(&entry.related_files).unwrap_or_default(),
                serde_json::to_string(&entry.related_inventory).unwrap_or_default(),
                entry.created_at.to_rfc3339(),
            ],
        ).map_err(|e| format!("portfolio_entries: {}", e))?;
    }

    // Playlists
    for pl in &config.playlists.playlists {
        conn.execute(
            "INSERT INTO playlists (id, name, description, created_by, tracks, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                pl.id, pl.name, pl.description, pl.created_by,
                serde_json::to_string(&pl.tracks).unwrap_or_default(),
                pl.created_at, pl.updated_at,
            ],
        ).map_err(|e| format!("playlists: {}", e))?;
    }

    // User reports config
    conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('reports_enabled', 'false')",
        [],
    ).map_err(|e| format!("reports_enabled: {}", e))?;

    println!(
        "[LabNAS] Migrados: {} usuarios, {} chats TG, {} impresoras 3D, {} items inventario, {} tareas, {} eventos, {} notas, {} playlists, {} portfolio",
        config.web_users.len(),
        config.notifications.telegram_chats.len(),
        config.printers3d.len(),
        config.inventory.items.len(),
        config.tasks.tasks.len(),
        config.tasks.events.len(),
        config.notes.len(),
        config.playlists.playlists.len(),
        config.portfolio.entries.len(),
    );

    Ok(())
}

// ═══════════════════════════════════════
// Helper for handlers
// ═══════════════════════════════════════

pub async fn db_op<F, T>(pool: &DbPool, f: F) -> Result<T, (axum::http::StatusCode, String)>
where
    F: FnOnce(&Connection) -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    let pool = pool.clone();
    tokio::task::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| format!("DB pool: {}", e))?;
        f(&conn)
    })
    .await
    .map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Task join error: {}", e),
        )
    })?
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e))
}

/// Like db_op but allows specifying a custom status code on error
pub async fn db_op_status<F, T>(
    pool: &DbPool,
    f: F,
) -> Result<T, (axum::http::StatusCode, String)>
where
    F: FnOnce(&Connection) -> Result<T, (axum::http::StatusCode, String)> + Send + 'static,
    T: Send + 'static,
{
    let pool = pool.clone();
    tokio::task::spawn_blocking(move || {
        let conn = pool
            .get()
            .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("DB pool: {}", e)))?;
        f(&conn)
    })
    .await
    .map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Task join error: {}", e),
        )
    })?
}

/// Para tareas de fondo y el bot: corre `f` con una conexion en un hilo bloqueante,
/// asi SQLite no frena a los demas futuros del runtime. Nunca guardes la conexion
/// mas alla del closure (nada de `.await` con ella en la mano).
pub async fn bg<F, T>(pool: &DbPool, f: F) -> Result<T, String>
where
    F: FnOnce(&Connection) -> T + Send + 'static,
    T: Send + 'static,
{
    let pool = pool.clone();
    tokio::task::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| format!("DB pool: {}", e))?;
        Ok(f(&conn))
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

// ═══════════════════════════════════════
// Settings helpers
// ═══════════════════════════════════════

pub fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

/// Setting cifrado en la base (API keys)
pub fn get_secret_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT labnas_decrypt(value) FROM settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn set_secret_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, labnas_encrypt(?2))",
        params![key, value],
    )
    .map_err(|e| format!("Error guardando setting {}: {}", key, e))?;
    Ok(())
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        params![key, value],
    )
    .map_err(|e| format!("Error guardando setting {}: {}", key, e))?;
    Ok(())
}

pub fn get_setting_bool(conn: &Connection, key: &str) -> bool {
    get_setting(conn, key)
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

pub fn get_setting_u32(conn: &Connection, key: &str, default: u32) -> u32 {
    get_setting(conn, key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

// ═══════════════════════════════════════
// Module helpers
// ═══════════════════════════════════════

/// Agrega los modulos que aun no estan en la base, con su estado por defecto: un modulo
/// nuevo aparece la primera vez que arranca la version que lo trae; despues manda el admin.
fn seed_modules(conn: &Connection) {
    for (order, m) in crate::modules::MODULES.iter().enumerate() {
        conn.execute(
            "INSERT OR IGNORE INTO modules (id, enabled, display_order) VALUES (?1, ?2, ?3)",
            params![m.id, m.default_on, order as i32],
        ).ok();
    }
}

/// Un modulo tal como lo ve la interfaz: lo del registro (`crate::modules`) mas el estado
/// que decide el admin
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModuleInfo {
    pub id: String,
    pub enabled: bool,
    pub display_order: i32,
    pub locked: bool,
    pub name: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
    pub path: &'static str,
    pub description: &'static str,
    pub lan_only: bool,
    /// Modulos que este usa
    pub uses: Vec<&'static str>,
}

pub fn get_modules(conn: &Connection) -> Vec<ModuleInfo> {
    let mut stmt = conn
        .prepare("SELECT id, enabled, display_order FROM modules ORDER BY display_order")
        .unwrap();
    stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?, row.get::<_, i32>(2)?)))
        .unwrap()
        .filter_map(|r| r.ok())
        // Un id que ya no esta en el registro (modulo retirado) no se muestra
        .filter_map(|(id, enabled, display_order)| {
            let m = crate::modules::get(&id)?;
            Some(ModuleInfo {
                locked: id == "dashboard",
                id,
                enabled,
                display_order,
                name: m.name,
                title: m.title.unwrap_or(m.name),
                icon: m.icon,
                path: m.path,
                description: m.description,
                lan_only: m.lan_only,
                uses: crate::modules::uses(m.id),
            })
        })
        .collect()
}

pub fn get_enabled_module_ids(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT id FROM modules WHERE enabled = 1 ORDER BY display_order")
        .unwrap();
    stmt.query_map([], |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
}

pub fn set_module_enabled(conn: &Connection, module_id: &str, enabled: bool) -> Result<(), String> {
    let affected = conn.execute(
        "UPDATE modules SET enabled = ?1 WHERE id = ?2",
        params![enabled, module_id],
    ).map_err(|e| format!("Error actualizando modulo {}: {}", module_id, e))?;
    if affected == 0 {
        return Err(format!("Modulo '{}' no existe", module_id));
    }
    Ok(())
}

pub fn set_module_order(conn: &Connection, modules: &[(String, i32)]) -> Result<(), String> {
    for (id, order) in modules {
        conn.execute(
            "UPDATE modules SET display_order = ?1 WHERE id = ?2",
            params![order, id],
        ).map_err(|e| format!("Error reordenando modulo {}: {}", id, e))?;
    }
    Ok(())
}

// ═══════════════════════════════════════
// Schema
// ═══════════════════════════════════════

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS branding (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    lab_name TEXT NOT NULL DEFAULT 'LabNAS',
    institution TEXT NOT NULL DEFAULT '',
    logo_url TEXT NOT NULL DEFAULT '',
    mission TEXT NOT NULL DEFAULT '',
    vision TEXT NOT NULL DEFAULT '',
    website TEXT NOT NULL DEFAULT '',
    contact_email TEXT NOT NULL DEFAULT '',
    location TEXT NOT NULL DEFAULT '',
    accent_color TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS web_users (
    username TEXT PRIMARY KEY,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'pendiente',
    perm_terminal INTEGER NOT NULL DEFAULT 0,
    perm_impresion INTEGER NOT NULL DEFAULT 0,
    perm_archivos_escritura INTEGER NOT NULL DEFAULT 0,
    linked_telegram INTEGER
);

CREATE TABLE IF NOT EXISTS notification_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    bot_token TEXT,
    bot_username TEXT,
    daily_enabled INTEGER NOT NULL DEFAULT 0,
    daily_hour INTEGER NOT NULL DEFAULT 8,
    daily_minute INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS telegram_chats (
    chat_id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    username TEXT,
    role TEXT NOT NULL DEFAULT 'pendiente',
    perm_terminal INTEGER NOT NULL DEFAULT 0,
    perm_impresion INTEGER NOT NULL DEFAULT 0,
    perm_archivos_escritura INTEGER NOT NULL DEFAULT 0,
    linked_web_user TEXT,
    daily_enabled INTEGER NOT NULL DEFAULT 0,
    daily_hour INTEGER NOT NULL DEFAULT 8,
    daily_minute INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS known_devices (
    mac TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    icon TEXT
);

CREATE TABLE IF NOT EXISTS services (
    port INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    icon TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS printer3d_sections (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    "order" INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS printers3d (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    ip TEXT NOT NULL,
    port INTEGER NOT NULL,
    printer_type TEXT NOT NULL,
    api_key TEXT,
    camera_url TEXT,
    power_watts REAL,
    electricity_cost_kwh REAL,
    section_id TEXT REFERENCES printer3d_sections(id) ON DELETE SET NULL,
    "order" INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS cups_printers (
    name TEXT PRIMARY KEY,
    ink_per_page REAL NOT NULL DEFAULT 0.0,
    paper_carta REAL NOT NULL DEFAULT 0.0,
    paper_oficio REAL NOT NULL DEFAULT 0.0,
    paper_special REAL NOT NULL DEFAULT 0.0,
    total_jobs INTEGER NOT NULL DEFAULT 0,
    total_pages INTEGER NOT NULL DEFAULT 0,
    pages_carta INTEGER NOT NULL DEFAULT 0,
    pages_oficio INTEGER NOT NULL DEFAULT 0,
    pages_special INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS cups_printer_user_stats (
    printer_name TEXT NOT NULL REFERENCES cups_printers(name) ON DELETE CASCADE,
    username TEXT NOT NULL,
    total_jobs INTEGER NOT NULL DEFAULT 0,
    total_pages INTEGER NOT NULL DEFAULT 0,
    pages_carta INTEGER NOT NULL DEFAULT 0,
    pages_oficio INTEGER NOT NULL DEFAULT 0,
    pages_special INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (printer_name, username)
);

CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_by TEXT NOT NULL,
    members TEXT NOT NULL DEFAULT '[]',
    member_tags TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    assigned_to TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'pendiente',
    created_by TEXT NOT NULL,
    due_date TEXT,
    due_time TEXT,
    requires_confirmation INTEGER NOT NULL DEFAULT 0,
    insistent INTEGER NOT NULL DEFAULT 0,
    reminder_minutes INTEGER NOT NULL DEFAULT 8,
    confirmed_by TEXT NOT NULL DEFAULT '[]',
    rejected_by TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    last_reminder TEXT
);

CREATE TABLE IF NOT EXISTS calendar_events (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    date TEXT NOT NULL,
    time TEXT NOT NULL,
    end_time TEXT,
    location TEXT,
    created_by TEXT NOT NULL,
    invitees TEXT NOT NULL DEFAULT '[]',
    accepted TEXT NOT NULL DEFAULT '[]',
    declined TEXT NOT NULL DEFAULT '[]',
    remind_before_min INTEGER NOT NULL DEFAULT 15,
    reminded INTEGER NOT NULL DEFAULT 0,
    notify_telegram INTEGER NOT NULL DEFAULT 1,
    recurrence TEXT NOT NULL DEFAULT '',
    recurrence_end TEXT,
    created_at TEXT NOT NULL,
    category TEXT
);

CREATE TABLE IF NOT EXISTS event_categories (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    color TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS notes (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    created_by TEXT NOT NULL,
    updated_by TEXT NOT NULL,
    shared_with TEXT NOT NULL DEFAULT '[]',
    is_public INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS email_accounts (
    username TEXT PRIMARY KEY,
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    protocol TEXT NOT NULL DEFAULT 'imap',
    email TEXT NOT NULL,
    password TEXT NOT NULL,
    filters TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE IF NOT EXISTS inventory_categories (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    icon TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS inventory_items (
    id TEXT PRIMARY KEY,
    category_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    quantity REAL NOT NULL DEFAULT 0.0,
    unit TEXT NOT NULL DEFAULT '',
    cost REAL NOT NULL DEFAULT 0.0,
    supplier TEXT NOT NULL DEFAULT '',
    supplier_url TEXT NOT NULL DEFAULT '',
    location TEXT NOT NULL DEFAULT '',
    serial_number TEXT NOT NULL DEFAULT '',
    purchase_date TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'activo',
    notes TEXT NOT NULL DEFAULT '',
    brand TEXT NOT NULL DEFAULT '',
    model_name TEXT NOT NULL DEFAULT '',
    material_type TEXT,
    filament_diameter REAL,
    filament_weight REAL,
    filament_remaining REAL,
    filament_color TEXT,
    filament_density REAL,
    custom_fields TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS print_history (
    id TEXT PRIMARY KEY,
    printer_id TEXT NOT NULL,
    file_name TEXT NOT NULL,
    filament_id TEXT,
    weight_grams REAL NOT NULL DEFAULT 0.0,
    print_time_minutes INTEGER NOT NULL DEFAULT 0,
    material_cost REAL NOT NULL DEFAULT 0.0,
    electricity_cost REAL NOT NULL DEFAULT 0.0,
    total_cost REAL NOT NULL DEFAULT 0.0,
    success INTEGER NOT NULL DEFAULT 0,
    notes TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS portfolio_entries (
    id TEXT PRIMARY KEY,
    entry_type TEXT NOT NULL DEFAULT 'project',
    scope TEXT NOT NULL DEFAULT 'own',
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    institution TEXT NOT NULL DEFAULT '',
    url TEXT NOT NULL DEFAULT '',
    contact TEXT NOT NULL DEFAULT '',
    funding_source TEXT NOT NULL DEFAULT '',
    budget REAL,
    principal_investigator TEXT NOT NULL DEFAULT '',
    collaborators TEXT NOT NULL DEFAULT '[]',
    participants TEXT NOT NULL DEFAULT '[]',
    start_date TEXT NOT NULL DEFAULT '',
    end_date TEXT,
    hours INTEGER,
    modality TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'planned',
    requirements TEXT NOT NULL DEFAULT '[]',
    milestones TEXT NOT NULL DEFAULT '[]',
    tags TEXT NOT NULL DEFAULT '[]',
    notes TEXT NOT NULL DEFAULT '',
    related_files TEXT NOT NULL DEFAULT '[]',
    related_inventory TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS playlists (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_by TEXT NOT NULL,
    tracks TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS user_reports (
    id TEXT PRIMARY KEY,
    report_type TEXT NOT NULL DEFAULT 'bug',
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    submitted_by TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pendiente',
    admin_response TEXT,
    created_at TEXT NOT NULL,
    responded_at TEXT
);

CREATE TABLE IF NOT EXISTS sensor_devices (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL DEFAULT '',
    mac TEXT NOT NULL UNIQUE,
    device_type TEXT NOT NULL DEFAULT '',
    connection TEXT NOT NULL DEFAULT 'espnow',
    status TEXT NOT NULL DEFAULT 'pending',
    battery INTEGER,
    rssi INTEGER,
    last_seen TEXT,
    config TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sensor_readings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id TEXT NOT NULL REFERENCES sensor_devices(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value REAL NOT NULL,
    unit TEXT NOT NULL DEFAULT '',
    timestamp TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sensor_readings_device_time
    ON sensor_readings(device_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_sensor_readings_device_key_time
    ON sensor_readings(device_id, key, timestamp);

CREATE TABLE IF NOT EXISTS sensor_alerts (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL REFERENCES sensor_devices(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    condition TEXT NOT NULL DEFAULT 'above',
    threshold REAL NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    cooldown_minutes INTEGER NOT NULL DEFAULT 30,
    last_triggered TEXT
);

CREATE TABLE IF NOT EXISTS modules (
    id TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL DEFAULT 1,
    display_order INTEGER NOT NULL DEFAULT 0
);
"#;
