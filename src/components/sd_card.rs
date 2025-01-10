use std::fs::{self, read_dir, File};
use std::io::{Read, Write};

use esp_idf_hal::{
    gpio::{InputPin, OutputPin},
    peripheral::Peripheral,
    spi::SpiAnyPins,
};
use esp_idf_svc::fs::fatfs::Fatfs;
use esp_idf_svc::hal::gpio::AnyIOPin;
use esp_idf_svc::hal::sd::{spi::SdSpiHostDriver, SdCardConfiguration, SdCardDriver};
use esp_idf_svc::hal::spi::{config::DriverConfig, Dma, SpiDriver};
use esp_idf_svc::io::vfs::MountedFatfs;

pub struct SdCard {
    _mounted_fs: MountedFatfs<Fatfs<SdCardDriver<SdSpiHostDriver<'static, SpiDriver<'static>>>>>,
}

impl SdCard {
    pub const SD_MOUNT_POINT: &'static str = "/sdcard";
    pub const DRINKS_DIRECTORY: &'static str = "/sdcard/drinks";
    pub fn new<SPI: SpiAnyPins>(
        spi: impl Peripheral<P = SPI> + 'static,
        sclk: impl Peripheral<P = impl OutputPin> + 'static,
        sdo: impl Peripheral<P = impl OutputPin> + 'static,
        sdi: impl Peripheral<P = impl InputPin> + 'static,
        cs: Option<impl Peripheral<P = impl OutputPin> + 'static>,
    ) -> anyhow::Result<Self> {
        log::info!("Starting up filesystem");

        let spi_driver = SpiDriver::new(
            spi,
            sclk,
            sdo,
            Some(sdi),
            &DriverConfig::default().dma(Dma::Auto(4096)),
        )?;

        log::info!("SPI driver created");

        let sd_card_driver = SdCardDriver::new_spi(
            SdSpiHostDriver::new(
                spi_driver,
                cs,
                AnyIOPin::none(),
                AnyIOPin::none(),
                AnyIOPin::none(),
                None,
            )?,
            &SdCardConfiguration::new(),
        )?;

        log::info!("SD card driver created");

        // Keep it around or else it will be dropped and unmounted
        let mounted_fatfs: MountedFatfs<Fatfs<SdCardDriver<SdSpiHostDriver<'_, SpiDriver<'_>>>>> =
            MountedFatfs::mount(
                Fatfs::new_sdcard(0, sd_card_driver)?,
                Self::SD_MOUNT_POINT,
                4,
            )
            .inspect_err(|e| {
                log::error!("Failed to mount filesystem: {}", e);
            })?;

        if !fs::exists(Self::DRINKS_DIRECTORY)? {
            log::info!("Creating {}", Self::DRINKS_DIRECTORY);
            fs::create_dir(Self::DRINKS_DIRECTORY).inspect_err(|e| {
                log::error!("Failed to create directory: {}", e);
            })?;
        }

        Ok(SdCard {
            _mounted_fs: mounted_fatfs,
        })
    }

    pub fn test() -> anyhow::Result<()> {
        log::info!("Mounted FATFS");
        let directory = read_dir(Self::SD_MOUNT_POINT)?;
        let test_file = format!("{}/test.txt", Self::SD_MOUNT_POINT);

        for entry in directory {
            log::info!("Entry: {:?}", entry?.file_name());
        }

        let content = b"Hello, world!";
        {
            let mut file = File::create(&test_file)?;
            log::info!("File {file:?} created");
            file.write_all(content).expect("Write failed");
            log::info!("File {file:?} written with {content:?}");
        }

        {
            let mut file = File::open(&test_file)?;
            log::info!("File {file:?} opened");
            let mut file_content = String::new();
            file.read_to_string(&mut file_content).expect("Read failed");
            log::info!("File {file:?} read: {file_content}");
            assert_eq!(file_content.as_bytes(), content);
        }

        {
            let directory = read_dir(Self::SD_MOUNT_POINT)?;
            for entry in directory {
                log::info!("Entry: {:?}", entry?.file_name());
            }
        }

        {
            fs::remove_file(&test_file)?;
            log::info!("File removed");
        }

        Ok(())
    }
}
