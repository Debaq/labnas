//! Tests de integracion: levantan el servidor completo (router, middleware, SQLite)
//! en proceso y lo usan por HTTP/WebSocket como lo haria la web.

mod harness;

mod backups;
mod bridge;
mod cameras;
mod email_ai;
mod email_oauth;
mod events;
mod finance_rendicion;
mod finance_compras;
mod ledger;
mod machines;
mod machine_usage;
mod maintenance;
mod deadlines;
mod display;
mod device_tokens;
mod files;
mod finance_presupuesto;
mod folders;
mod https;
mod inventory;
mod ipp;
mod network;
mod passkeys;
mod roles;
mod safety;
mod printers;
mod secrets;
mod security;
mod sensors;
mod scan;
mod setup;
mod shares;
mod twofa;
mod smart;
mod telegram;
mod access;
mod snapshots;
mod timelapse;
mod webdav;
mod workshops;
mod finance_transparencia;
mod finance_viajes;
