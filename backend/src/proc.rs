//! Comandos externos con limite de tiempo: un `yt-dlp`, `lp` o `ffmpeg` colgado (red
//! caida, impresora trabada) no puede dejar un pedido o una tarea esperando para siempre.
//! Al vencer el plazo el proceso se mata.

use std::io::{Error, ErrorKind, Read};
use std::process::{Output, Stdio};
use std::time::{Duration, Instant};

/// Buscar o consultar un video no deberia tardar mas
pub const YTDLP_TIMEOUT: Duration = Duration::from_secs(60);
/// Encolar un trabajo en CUPS es inmediato; si tarda, CUPS esta trabado
pub const LP_TIMEOUT: Duration = Duration::from_secs(60);
/// Armar un timelapse largo (miles de fotogramas) en un equipo modesto
pub const FFMPEG_TIMELAPSE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

fn timed_out(limit: Duration) -> Error {
    Error::new(ErrorKind::TimedOut, format!("no termino en {} s y se detuvo", limit.as_secs()))
}

/// Como `Command::output()` (tokio), pero mata el proceso si no termina en `limit`
pub async fn output_timeout(cmd: &mut tokio::process::Command, limit: Duration) -> std::io::Result<Output> {
    cmd.kill_on_drop(true);
    match tokio::time::timeout(limit, cmd.output()).await {
        Ok(r) => r,
        // Al soltar el futuro se suelta el hijo, y kill_on_drop lo mata
        Err(_) => Err(timed_out(limit)),
    }
}

/// Version bloqueante (para codigo que ya corre en `spawn_blocking` o en un hilo)
pub fn output_timeout_blocking(cmd: &mut std::process::Command, limit: Duration) -> std::io::Result<Output> {
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    // Leer las salidas en paralelo: si se llena un pipe, el hijo se bloquea
    let read_all = |r: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut r) = r {
                let _ = r.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out = read_all(child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let err = read_all(child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>));

    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(timed_out(limit));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    Ok(Output { status, stdout: out.join().unwrap_or_default(), stderr: err.join().unwrap_or_default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn async_respeta_el_limite() {
        let t = Instant::now();
        let e = output_timeout(tokio::process::Command::new("sleep").arg("5"), Duration::from_millis(200)).await.unwrap_err();
        assert_eq!(e.kind(), ErrorKind::TimedOut);
        assert!(t.elapsed() < Duration::from_secs(2));
        let ok = output_timeout(tokio::process::Command::new("echo").arg("hola"), Duration::from_secs(5)).await.unwrap();
        assert_eq!(String::from_utf8_lossy(&ok.stdout), "hola\n");
    }

    #[test]
    fn bloqueante_respeta_el_limite_y_lee_salidas_grandes() {
        let t = Instant::now();
        let e = output_timeout_blocking(std::process::Command::new("sleep").arg("5"), Duration::from_millis(200)).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::TimedOut);
        assert!(t.elapsed() < Duration::from_secs(2));
        // Mas que el buffer de un pipe (64 KiB): no se traba
        let ok = output_timeout_blocking(
            std::process::Command::new("sh").args(["-c", "head -c 300000 /dev/zero; echo fin >&2"]),
            Duration::from_secs(10),
        )
        .unwrap();
        assert!(ok.status.success());
        assert_eq!((ok.stdout.len(), String::from_utf8_lossy(&ok.stderr).trim()), (300_000, "fin"));
    }
}
