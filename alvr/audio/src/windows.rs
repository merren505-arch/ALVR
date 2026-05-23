use crate::AudioDevice;
use alvr_common::anyhow::{bail, Result};
use rodio::DeviceTrait;
use windows::{
    core::{Interface, GUID},
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Media::Audio::{
            eAll, eRender, Endpoints::IAudioEndpointVolume, IMMDevice, IMMDeviceEnumerator,
            IMMEndpoint, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
        },
        System::Com::{self, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ},
    },
};

// RAII guard to manage the lifetime of the COM library on this thread
struct ComGuard(bool);

impl ComGuard {
    fn new() -> Self {
        unsafe {
            // Initialize COM as multi-threaded. S_OK or S_FALSE means it succeeded
            let hr = Com::CoInitializeEx(None, COINIT_MULTITHREADED);
            Self(hr.is_ok())
        }
    }
}

// Automatically uninitialize COM when the guard goes out of scope
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { Com::CoUninitialize() };
        }
    }
}

fn get_windows_device(device: &AudioDevice) -> Result<IMMDevice> {
    let device_name = device.inner.name()?;

    // Ensure the device name is not empty
    assert!(!device_name.is_empty());

    // Initialize COM for the duration of this function
    let _com_guard = ComGuard::new();

    unsafe {
        let imm_device_enumerator: IMMDeviceEnumerator =
            Com::CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

        let imm_device_collection =
            imm_device_enumerator.EnumAudioEndpoints(eAll, DEVICE_STATE_ACTIVE)?;

        // Get the total number of audio endpoints
        let device_count = imm_device_collection.GetCount()?;
        assert!(device_count >= 0);

        for i in 0..device_count {
            // Get the IMMDevice at the current index
            let imm_device = imm_device_collection.Item(i)?;

            // Query the friendly name of the device
            let imm_device_name = imm_device
                .OpenPropertyStore(STGM_READ)?
                .GetValue(&PKEY_Device_FriendlyName)?
                .to_string();

            // Check if the device is configured as an output device
            let is_output = imm_device.cast::<IMMEndpoint>()?.GetDataFlow()? == eRender;

            if imm_device_name == device_name && device.is_output == is_output {
                return Ok(imm_device);
            }
        }

        bail!("No device found with specified name")
    }
}

pub fn get_windows_device_id(device: &AudioDevice) -> Result<String> {
    // Verify the device name is valid before querying its ID
    assert!(device.inner.name().is_ok());

    unsafe {
        let imm_device = get_windows_device(device)?;

        let id_str_ptr = imm_device.GetId()?;
        
        // Ensure the ID string pointer returned is valid
        assert!(!id_str_ptr.is_null());

        let id_str = id_str_ptr.to_string()?;
        Com::CoTaskMemFree(Some(id_str_ptr.0 as _));

        Ok(id_str)
    }
}

// device must be an output device
pub fn set_mute_windows_device(device: &AudioDevice, mute: bool) -> Result<()> {
    assert!(device.is_output);
    assert!(device.inner.name().is_ok());

    unsafe {
        let imm_device = get_windows_device(device)?;

        let endpoint_volume = imm_device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)?;

        endpoint_volume.SetMute(mute, &GUID::zeroed())?;
    }

    Ok(())
}
