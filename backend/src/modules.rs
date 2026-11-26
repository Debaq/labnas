//! Registro unico de modulos: todo lo que el resto del sistema necesita saber de cada
//! modulo (nombre, icono, ruta en la interfaz, rutas de la API, si viene encendido, que
//! otros modulos usa). Agregar un modulo es agregar una entrada aca, mas su pagina en
//! `frontend/src/modules.tsx`, su nivel en cada plantilla de rol (`roles::template_level`)
//! y, si tiene, sus tareas de fondo (`module_tasks`).
//!
//! El orden de la lista es el orden con que aparece en el menu la primera vez.

pub struct Module {
    pub id: &'static str,
    /// Nombre corto (menu, ajustes, roles, Telegram)
    pub name: &'static str,
    /// Titulo de su pagina, si es distinto del nombre
    pub title: Option<&'static str>,
    /// Icono de lucide-react (el frontend lo traduce en `lib/moduleIcons.ts`)
    pub icon: &'static str,
    /// Ruta de su pagina en la interfaz
    pub path: &'static str,
    pub description: &'static str,
    /// Encendido al instalarse (despues manda el admin)
    pub default_on: bool,
    /// Prefijos de la API que pertenecen al modulo: el middleware exige el nivel del rol en el
    /// modulo y lo bloquea si esta apagado
    pub api: &'static [&'static str],
    /// Rutas sin sesion. Terminada en `/` es prefijo; si no, ruta exacta
    pub public: &'static [&'static str],
    /// Solo disponible en la red del laboratorio (no por el puente)
    pub lan_only: bool,
    /// Otros modulos que usa y para que. Sin ellos sigue funcionando, pero pierde esa parte:
    /// apagar uno que otro usa pide confirmacion
    pub uses: &'static [(&'static str, &'static str)],
}

const fn m(id: &'static str, name: &'static str, icon: &'static str, description: &'static str) -> Module {
    Module {
        id,
        name,
        title: None,
        icon,
        path: "",
        description,
        default_on: true,
        api: &[],
        public: &[],
        lan_only: false,
        uses: &[],
    }
}

pub const MODULES: &[Module] = &[
    Module {
        path: "/dashboard",
        ..m("dashboard", "Dashboard", "LayoutDashboard", "Panel principal con estadisticas y resumen")
    },
    Module {
        title: Some("Explorador de Archivos"),
        path: "/files",
        api: &["/api/files", "/api/shares", "/api/share/", "/api/download-url", "/api/trash", "/api/preview/"],
        // enlaces publicos y vistas previas (token de un solo archivo, ver files::serve_preview)
        public: &["/api/share/", "/api/preview/"],
        ..m("files", "Archivos", "FolderOpen", "Explorador y gestor de archivos")
    },
    Module {
        title: Some("Red Local"),
        path: "/network",
        api: &["/api/network"],
        ..m("network", "Red", "Network", "Escaneo y monitoreo de red local")
    },
    Module {
        title: Some("Impresion de Documentos"),
        path: "/printing",
        api: &["/api/printing"],
        ..m("printing", "Impresion", "Printer", "Impresion de documentos via CUPS")
    },
    Module {
        path: "/printers3d",
        api: &["/api/printers3d"],
        ..m("printers3d", "Impresoras 3D", "Box", "Control de impresoras 3D (OctoPrint/Moonraker)")
    },
    Module {
        path: "/tasks",
        api: &["/api/tasks", "/api/projects", "/api/events"],
        ..m("tasks", "Tareas / Horario", "ClipboardList", "Tareas, proyectos y calendario")
    },
    Module {
        path: "/notes",
        api: &["/api/notes"],
        ..m("notes", "Notas", "FileText", "Notas compartidas del laboratorio")
    },
    Module {
        path: "/email",
        api: &["/api/email"],
        uses: &[("tasks", "agendar las propuestas de la IA")],
        ..m("email", "Correo", "Mail", "Correo electronico IMAP")
    },
    Module {
        path: "/inventory",
        api: &["/api/inventory"],
        ..m("inventory", "Inventario", "Package", "Inventario de equipos y materiales")
    },
    Module {
        title: Some("Portafolio Academico"),
        path: "/portfolio",
        api: &["/api/portfolio"],
        // inscripcion a talleres por enlace secreto y verificacion de certificados (handlers::workshops)
        public: &["/api/portfolio/taller/", "/api/portfolio/certificado/"],
        ..m("portfolio", "Portafolio", "GraduationCap", "Portafolio academico e investigacion")
    },
    Module {
        path: "/sensors",
        api: &["/api/sensors"],
        // lecturas que mandan los sensores WiFi
        public: &["/api/sensors/data"],
        ..m("sensors", "Sensores", "Thermometer", "Sensores IoT (ESP-NOW / WiFi)")
    },
    Module {
        path: "/terminal",
        api: &["/api/terminal"],
        lan_only: true,
        ..m("terminal", "Terminal", "TerminalSquare", "Terminal web remoto")
    },
    Module {
        // Sin pagina propia: panel en la barra lateral y editor de playlists
        api: &["/api/music"],
        ..m("music", "Musica", "Music", "Reproductor de musica (yt-dlp + mpv)")
    },
    Module {
        path: "/cameras",
        api: &["/api/cameras"],
        // cuadro para Obico: token aleatorio de un solo uso (cameras::obico)
        public: &["/api/cameras/frame/"],
        lan_only: true,
        // apagado hasta que el admin lo active (privacidad)
        default_on: false,
        uses: &[("printers3d", "ver y grabar las camaras de las impresoras 3D")],
        ..m("cameras", "Camaras", "Video", "Webcams del servidor y camaras IP: vista, fotos y grabaciones (viene apagado)")
    },
    Module {
        path: "/machines",
        api: &["/api/machines"],
        ..m("machines", "Maquinas", "Wrench", "Catalogo de maquinas con QR, reservas sin choques y reporte de fallas")
    },
    Module {
        path: "/safety",
        api: &["/api/safety"],
        uses: &[
            ("machines", "vincular incidentes y fichas a una maquina"),
            ("inventory", "vincular fichas a un item del inventario"),
        ],
        ..m("seguridad", "Seguridad", "ShieldAlert", "Capacitaciones vigentes, fichas de seguridad y registro de incidentes y casi-accidentes")
    },
    Module {
        path: "/deadlines",
        api: &["/api/deadlines"],
        uses: &[("portfolio", "mostrar los plazos de los proyectos del Portafolio")],
        ..m("deadlines", "Plazos", "Hourglass", "Cuenta regresiva de cierres de proyecto, informes, rendiciones y otros plazos, con avisos por Telegram")
    },
    Module {
        path: "/finanzas",
        api: &["/api/finance"],
        uses: &[
            ("portfolio", "los proyectos con presupuesto son los del Portafolio"),
            ("inventory", "bienes comprados por proyecto"),
            ("deadlines", "hitos, rendiciones y plazos de pago en Plazos"),
        ],
        ..m("finanzas", "Finanzas", "Landmark", "Presupuesto por proyecto, compras con funciones separadas, rendiciones y expediente para auditorias")
    },
    Module {
        path: "/transparencia",
        api: &["/api/transparency"],
        // portal y solicitudes de informacion, sin cuenta
        public: &["/api/transparency/public/"],
        // apagado hasta que el admin lo active: publica datos hacia afuera
        default_on: false,
        uses: &[("finanzas", "proyectos, montos y bienes que se publican")],
        ..m("transparencia", "Transparencia", "Eye", "Portal publico generado desde los datos reales y solicitudes de informacion con plazo (viene apagado)")
    },
];

pub fn get(id: &str) -> Option<&'static Module> {
    MODULES.iter().find(|m| m.id == id)
}

/// Ids de todos los modulos conocidos
pub fn ids() -> Vec<&'static str> {
    MODULES.iter().map(|m| m.id).collect()
}

/// Nombre visible de un modulo (el id si no se conoce)
pub fn name(id: &str) -> &str {
    get(id).map(|m| m.name).unwrap_or(id)
}

/// Modulos que usa `id`
pub fn uses(id: &str) -> Vec<&'static str> {
    get(id).map(|m| m.uses.iter().map(|(u, _)| *u).collect()).unwrap_or_default()
}

/// Modulo al que pertenece una ruta de la API
pub fn for_path(path: &str) -> Option<&'static str> {
    MODULES
        .iter()
        .find(|m| m.api.iter().any(|p| path.starts_with(p)))
        .map(|m| m.id)
}

/// Ruta publica (sin sesion) declarada por algun modulo
pub fn is_public(path: &str) -> bool {
    MODULES.iter().flat_map(|m| m.public.iter()).any(|p| {
        if p.ends_with('/') {
            path.starts_with(p)
        } else {
            path == *p
        }
    })
}

/// Aviso si apagar `id` le quita algo a otro modulo encendido (None = nada que avisar)
pub fn disable_warning(id: &str, enabled: &std::collections::HashSet<String>) -> Option<String> {
    let users: Vec<String> = MODULES
        .iter()
        .filter(|m| enabled.contains(m.id))
        .flat_map(|m| m.uses.iter().filter(|(u, _)| *u == id).map(move |(_, why)| format!("{} lo usa para {}", m.name, why)))
        .collect();
    (!users.is_empty()).then(|| users.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registro_coherente() {
        let ids = ids();
        let mut seen = std::collections::HashSet::new();
        for m in MODULES {
            assert!(seen.insert(m.id), "id repetido: {}", m.id);
            for (u, _) in m.uses {
                assert!(ids.contains(u) && *u != m.id, "{} usa {u}", m.id);
            }
            for p in m.public {
                assert!(m.api.iter().any(|a| p.starts_with(a)), "{}: ruta publica {p} fuera de su API", m.id);
            }
            assert!(m.path.is_empty() || m.path.starts_with('/'), "{}", m.id);
        }
        // Un prefijo de la API no puede pertenecer a dos modulos
        for a in MODULES {
            for b in MODULES.iter().filter(|b| b.id != a.id) {
                for pa in a.api {
                    assert!(!b.api.iter().any(|pb| pa.starts_with(pb)), "{pa} ({}) cae en {}", a.id, b.id);
                }
            }
        }
    }

    #[test]
    fn rutas_y_publicas() {
        assert_eq!(for_path("/api/files/list"), Some("files"));
        assert_eq!(for_path("/api/events"), Some("tasks"));
        assert_eq!(for_path("/api/settings"), None);
        assert!(is_public("/api/share/abc"));
        assert!(is_public("/api/sensors/data"));
        assert!(!is_public("/api/sensors/data2"));
        assert!(!is_public("/api/sensors"));
        assert!(!is_public("/api/cameras/frame"));
    }

    #[test]
    fn avisa_si_otro_modulo_lo_usa() {
        let on: std::collections::HashSet<String> = ["email".to_string(), "tasks".to_string()].into();
        assert!(disable_warning("tasks", &on).unwrap().contains("Correo"));
        assert!(disable_warning("email", &on).is_none());
        // Plazos apagado: apagar Portafolio no le quita nada a nadie
        assert!(disable_warning("portfolio", &on).is_none());
    }
}
