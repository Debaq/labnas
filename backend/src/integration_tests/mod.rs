//! Tests de integracion: levantan el servidor completo (router, middleware, SQLite)
//! en proceso y lo usan por HTTP/WebSocket como lo haria la web.

mod harness;

mod backups;
mod bridge;
mod email_ai;
mod email_oauth;
mod events;
mod files;
mod folders;
mod https;
mod network;
mod printers;
mod secrets;
mod security;
mod sensors;
mod setup;
mod shares;
mod twofa;
mod smart;
mod timelapse;
mod webdav;
