//! Tests de integracion: levantan el servidor completo (router, middleware, SQLite)
//! en proceso y lo usan por HTTP/WebSocket como lo haria la web.

mod harness;

mod backups;
mod bridge;
mod cameras;
mod email_ai;
mod email_oauth;
mod events;
mod ledger;
mod machines;
mod files;
mod folders;
mod https;
mod ipp;
mod network;
mod passkeys;
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
mod snapshots;
mod timelapse;
mod webdav;
