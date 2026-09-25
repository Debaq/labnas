//! Tests de integracion: levantan el servidor completo (router, middleware, SQLite)
//! en proceso y lo usan por HTTP/WebSocket como lo haria la web.

mod harness;

mod backups;
mod events;
mod files;
mod folders;
mod network;
mod printers;
mod secrets;
mod security;
mod sensors;
mod webdav;
