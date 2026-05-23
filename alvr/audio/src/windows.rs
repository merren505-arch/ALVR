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

// RAII guard to safely manage COM library initialization and uninitialization on the current thread
struct ComGuard(bool);

impl ComGuard {
    fn new() -> Self {
        unsafe {
            // Initialize COM with multithreaded concurrency. S_OK and S_FALSE represent success
            let hr = Com::CoInitializeEx(None, COINIT_MULTITHREADED);
            Self(hr.is_ok())
        }
    }
}

// Automatically release COM resources if they were successfully initialized
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { Com::CoUninitialize() };
        }
    }
}

fn get_windows_device(device: &AudioDevice) -> Result<IMMDevice> {
    let device_name = device.inner.name()?;

    // Check preconditions to ensure the device has a valid name
    assert!(!device_name.is_empty());

    let _com_guard = ComGuard::new();

    unsafe {
        let imm_device_enumerator: IMMDeviceEnumerator =
            Com::CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

        let imm_device_collection =
            imm_device_enumerator.EnumAudioEndpoints(eAll, DEVICE_STATE_ACTIVE)?;

        // Get the total number of audio endpoints currently active
        let device_count = imm_device_collection.GetCount()?;
        
        // Ensure device count is valid and non-negative
        assert!(device_count >= 0);

        for i in 0..device_count {
            let imm_device = imm_device_collection.Item(i)?;

            // Query the friendly name of each endpoint device
            let imm_device_name = imm_device
                .OpenPropertyStore(STGM_READ)?
                .GetValue(&PKEY_Device_FriendlyName)?
                .to_string();

            // Determine if the endpoint matches our target data flow
            let is_output = imm_device.cast::<IMMEndpoint>()?.GetDataFlow()? == eRender;

            if imm_device_name == device_name && device.is_output == is_output {
                return Ok(imm_device);
            }
        }

        bail!("No device found with specified name")
    }
}

pub fn get_windows_device_id(device: &AudioDevice) -> Result<String> {
    assert!(device.inner.name().is_ok());

    unsafe {
        let imm_device = get_windows_device(device)?;

        let id_str_ptr = imm_device.GetId()?;
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
