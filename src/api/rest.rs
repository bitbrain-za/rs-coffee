use crate::app_state::ApiState;
use crate::schemas::drink::Drink;
use embedded_svc::{
    http::{Headers, Method},
    io::{Read, Write},
};
use esp_idf_svc::http::server::EspHttpServer;

const STACK_SIZE: usize = 1024 * 10;
const MAX_LEN: usize = 2048;
const VERSION: &str = env!("CARGO_PKG_VERSION");

macro_rules! handle_request_data {
    ($req:expr) => {{
        let len = $req.content_len().unwrap_or(0) as usize;

        if len > MAX_LEN {
            $req.into_status_response(413)?
                .write_all("Request too big".as_bytes())?;
            return Ok(());
        }
        let mut buf = vec![0; len];
        $req.read_exact(&mut buf)?;
        String::from_utf8(buf).unwrap()
    }};
}

macro_rules! success {
    () => {
        Ok(serde_json::json!({ "status": "success" }).to_string())
    };
}

macro_rules! ok {
    ($req:expr, $resp:expr) => {{
        $req.into_ok_response()?.write_all($resp.as_bytes())?;
    }};
}

macro_rules! bad_request {
    ($req:expr, $err:expr) => {{
        $req.into_status_response(400)?
            .write_all($err.to_string().as_bytes())?;
    }};
}

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
        let data = handle_request_data!(req);
        echo_post(&data, my_system.clone());
        req.into_ok_response()?.write_all("done".as_bytes())?;
        Ok(())
    })?;

    let my_system = system.clone();
    server.fn_handler::<anyhow::Error, _>(
        "/api/v1/coffee/drink",
        Method::Put,
        move |mut req| {
            let data = handle_request_data!(req);
            match put_drink(&data, my_system.clone()) {
                Ok(message) => ok!(req, message),
                Err(e) => bad_request!(req, e),
            }
            Ok(())
        },
    )?;

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

fn put_drink(data: &str, system: ApiState) -> Result<String, String> {
    let drink: Drink = serde_json::from_str(data).map_err(|e| e.to_string())?;
    drink.validate().map_err(|e| e.to_string())?;
    system.lock().unwrap().drink = Some(drink);
    success!()
}
