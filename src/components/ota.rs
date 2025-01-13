use embedded_svc::http::{client::Client as HttpClient, Method};
use esp_idf_svc::http::client::EspHttpConnection;
use esp_idf_svc::ota::EspOta;

pub fn get_request() -> anyhow::Result<()> {
    const FIRMWARE_DOWNLOAD_CHUNK_SIZE: usize = 1024 * 2;
    const FIRMWARE_MAX_SIZE: usize = 1024 * 1024 * 4;
    let url = dotenv_codegen::dotenv!("FW_UPDATE_URL");

    let mut client = HttpClient::wrap(EspHttpConnection::new(&Default::default())?);

    let headers = [(
        http::header::ACCEPT.as_str(),
        mime::APPLICATION_OCTET_STREAM.as_ref(),
    )];
    let request = client.request(Method::Get, url, &headers)?;
    let mut response = request.submit()?;

    let status = response.status();
    if 200 != status {
        log::error!("Bad HTTP response: {}", status);
        return Err(anyhow::anyhow!("Bad HTTP response: {}", status));
    }
    let mut buf = [0u8; FIRMWARE_DOWNLOAD_CHUNK_SIZE];

    let (headers, stream) = response.split();

    let mut total_read_len: usize = 0;
    let file_size = headers
        .header("Content-Length")
        .unwrap_or_default()
        .parse::<usize>()?;
    log::debug!("File size: {}", file_size);

    if file_size > FIRMWARE_MAX_SIZE {
        log::error!("File is too big ({file_size} bytes).");
        return Err(anyhow::anyhow!("File is too big ({file_size} bytes)."));
    }

    let mut ota = EspOta::new()?;
    let mut update = ota.initiate_update()?;

    loop {
        let n = stream.read(&mut buf).unwrap_or_default();
        total_read_len += n;

        log::info!("Read {} bytes of {}", total_read_len, file_size);

        update.write(&buf[..n]).expect("write OTA data");

        if total_read_len >= file_size {
            break;
        }
    }
    // [ ] check the file is okay before completing;
    update.complete()?;
    esp_idf_svc::hal::reset::restart();
}
