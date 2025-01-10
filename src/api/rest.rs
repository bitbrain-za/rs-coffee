use super::{handlers_device, handlers_drinks};
use crate::app_state::System;
use anyhow::{Error, Result};
use embedded_svc::{
    http::{Headers, Method},
    io::{Read, Write},
};
use esp_idf_svc::http::server::EspHttpServer;

const STACK_SIZE: usize = 1024 * 10;
const MAX_LEN: usize = 2048;

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

macro_rules! ok {
    ($req:expr) => {{
        let result = serde_json::json!({ "status": "success"}).to_string();
        $req.into_ok_response()?.write_all(result.as_bytes())?;
        Ok(())
    }};
}

macro_rules! ok_with_text {
    ($req:expr, $resp:expr) => {{
        let result = serde_json::json!({ "status": "success", "message": $resp }).to_string();
        $req.into_ok_response()?.write_all(result.as_bytes())?;
        Ok(())
    }};
}

macro_rules! ok_with_json {
    ($req:expr, $resp:expr) => {{
        let result = serde_json::to_string_pretty(&$resp)?;
        $req.into_ok_response()?.write_all(result.as_bytes())?;
        Ok(())
    }};
}

macro_rules! bad_request {
    ($req:expr, $err:expr) => {{
        $req.into_status_response(400)?
            .write_all($err.to_string().as_bytes())?;
        Ok(())
    }};
}

pub fn create_server(system: System) -> Result<EspHttpServer<'static>> {
    let server_configuration = esp_idf_svc::http::server::Configuration {
        stack_size: STACK_SIZE,
        ..Default::default()
    };

    let mut server = EspHttpServer::new(&server_configuration)?;
    create_router(&mut server, system)?;
    Ok(server)
}

fn create_router(server: &mut EspHttpServer<'static>, system: System) -> Result<()> {
    /* Device Endpoints */
    server.fn_handler::<Error, _>("/api/v1/version", Method::Get, |req| {
        let resp = handlers_device::version();
        ok_with_text!(req, resp)
    })?;

    let my_system = system.clone();
    server.fn_handler::<Error, _>("/api/v1/echo", Method::Get, move |req| {
        match handlers_device::echo_get(my_system.clone()) {
            Ok(data) => ok_with_text!(req, data),
            Err(e) => bad_request!(req, e),
        }
    })?;

    let my_system = system.clone();
    server.fn_handler::<Error, _>("/api/v1/echo", Method::Post, move |mut req| {
        let data = handle_request_data!(req);
        handlers_device::echo_post(&data, my_system.clone());
        ok!(req)
    })?;

    /* Drink Endpoints */
    let my_system = system.clone();
    server.fn_handler::<Error, _>("/api/v1/coffee/drink", Method::Get, move |req| {
        log::info!("Request: {:?}", req.uri());
        let data = req.uri().to_string();
        match handlers_drinks::get_drink(&data, my_system.clone()) {
            Ok(json) => ok_with_json!(req, json),
            Err(e) => bad_request!(req, e),
        }
    })?;

    let my_system = system.clone();
    server.fn_handler::<Error, _>("/api/v1/coffee/drink", Method::Put, move |mut req| {
        let data = handle_request_data!(req);
        match handlers_drinks::put_drink(&data, my_system.clone()) {
            Ok(message) => ok_with_text!(req, message),
            Err(e) => bad_request!(req, e),
        }
    })?;

    let my_system = system.clone();
    server.fn_handler::<Error, _>("/api/v1/coffee/drink", Method::Post, move |mut req| {
        let data = handle_request_data!(req);
        match handlers_drinks::post_drink(&data, my_system.clone()) {
            Ok(message) => ok_with_text!(req, message),
            Err(e) => bad_request!(req, e),
        }
    })?;

    let my_system = system.clone();
    server.fn_handler::<Error, _>("/api/v1/device/config", Method::Get, move |req| {
        match handlers_device::get_config(my_system.clone()) {
            Ok(data) => ok_with_json!(req, data),
            Err(e) => bad_request!(req, e),
        }
    })?;

    let my_system = system.clone();
    server.fn_handler::<Error, _>("/api/v1/device/config", Method::Put, move |mut req| {
        let data = handle_request_data!(req);
        match handlers_device::set_config(&data, my_system.clone()) {
            Ok(value) => ok_with_json!(req, value),
            Err(e) => bad_request!(req, e),
        }
    })?;

    Ok(())
}
