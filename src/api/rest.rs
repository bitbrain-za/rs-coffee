use crate::app_state::ApiState;
use embedded_svc::{
    http::{Headers, Method},
    io::{Read, Write},
};
use esp_idf_svc::http::server::EspHttpServer;

const STACK_SIZE: usize = 1024 * 10;
const MAX_LEN: usize = 128;
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn create_server(system: ApiState) -> anyhow::Result<EspHttpServer<'static>> {
    let server_configuration = esp_idf_svc::http::server::Configuration {
        stack_size: STACK_SIZE,
        ..Default::default()
    };

    let mut server = EspHttpServer::new(&server_configuration)?;
    create_router(&mut server, system)?;
    Ok(server)
}

fn create_router(server: &mut EspHttpServer<'static>, system: ApiState) -> anyhow::Result<()> {
    server.fn_handler("/api/v1/version", Method::Get, |req| {
        let resp = version();
        req.into_ok_response()?
            .write_all(resp.as_bytes())
            .map(|_| ())
    })?;

    let my_system = system.clone();
    server.fn_handler("/api/v1/echo", Method::Get, move |req| {
        let resp = echo_get(my_system.clone());
        req.into_ok_response()?
            .write_all(resp.as_bytes())
            .map(|_| ())
    })?;

    let my_system = system.clone();
    server.fn_handler::<anyhow::Error, _>("/api/v1/echo", Method::Post, move |mut req| {
        let len = req.content_len().unwrap_or(0) as usize;

        if len > MAX_LEN {
            req.into_status_response(413)?
                .write_all("Request too big".as_bytes())?;
            return Ok(());
        }
        let mut buf = vec![0; len];
        req.read_exact(&mut buf)?;
        let data: String = String::from_utf8(buf).unwrap();
        echo_post(&data, my_system.clone());
        req.into_ok_response()?.write_all("done".as_bytes())?;
        Ok(())
    })?;

    Ok(())
}

fn version() -> &'static str {
    VERSION
}

fn echo_post(data: &str, system: ApiState) -> String {
    system.lock().unwrap().echo_data = data.to_string();
    "done".to_string()
}

fn echo_get(system: ApiState) -> String {
    system.lock().unwrap().echo_data.clone()
}
