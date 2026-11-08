//! Dispatch vendor protocols before any device bytes are sent.
use crate::{device_controls::DeviceControlConfirmation, discovery, oneplus, protocol, transport};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

pub enum Session {
    Sony(discovery::ReadSession),
    OnePlus(oneplus::Session),
}

impl From<discovery::ReadSession> for Session {
    fn from(value: discovery::ReadSession) -> Self {
        Self::Sony(value)
    }
}

pub fn read_basics(
    device: &transport::PairedDevice,
    connection: &mut dyn transport::Connection,
    cancelled: &AtomicBool,
) -> Result<(discovery::BasicInfo, Session), String> {
    if crate::oneplus_models::is_oneplus_name(&device.model)
        || crate::oneplus_models::is_oneplus_name(&device.name)
    {
        let (info, session) = oneplus::read_basics(connection, cancelled)?;
        Ok((info, Session::OnePlus(session)))
    } else if transport::is_sony_headphone(&device.name) {
        let (info, session) = discovery::read_basics(connection, cancelled);
        Ok((info, Session::Sony(session)))
    } else {
        Err("No control adapter is available for this device.".into())
    }
}

impl Session {
    pub fn is_synchronized(&self) -> bool {
        match self {
            Self::Sony(session) => session.is_synchronized(),
            Self::OnePlus(session) => session.is_synchronized(),
        }
    }
    pub fn metadata(&self) -> BTreeMap<String, serde_json::Value> {
        match self {
            Self::Sony(_) => BTreeMap::new(),
            Self::OnePlus(session) => session.metadata(),
        }
    }
    pub fn drain_metadata_updates(&mut self) -> BTreeMap<String, serde_json::Value> {
        match self {
            Self::Sony(_) => BTreeMap::new(),
            Self::OnePlus(session) => session.drain_metadata_updates(),
        }
    }
    pub fn feed_frames(
        &mut self,
        bytes: &[u8],
    ) -> Vec<Result<protocol::Frame, protocol::CodecError>> {
        match self {
            Self::Sony(session) => session.feed_frames(bytes),
            Self::OnePlus(session) => {
                session.feed(bytes);
                vec![]
            }
        }
    }
    pub fn drain_noise_observations(&mut self) -> Vec<Vec<u8>> {
        match self {
            Self::Sony(session) => session.drain_noise_observations(),
            Self::OnePlus(session) => session.drain_noise_observations(),
        }
    }
    pub fn drain_device_updates(&mut self) -> Vec<DeviceControlConfirmation> {
        match self {
            Self::Sony(session) => session.drain_device_updates(),
            Self::OnePlus(session) => session.drain_device_updates(),
        }
    }
    pub fn parse_noise_notification(&self, data: &[u8]) -> Option<discovery::NoiseInfo> {
        match self {
            Self::Sony(session) => session.parse_noise_notification(data),
            Self::OnePlus(session) => session.parse_noise_notification(data),
        }
    }
    pub fn apply_setting(
        &mut self,
        connection: &mut dyn transport::Connection,
        cancelled: &AtomicBool,
        change: discovery::SettingChange,
    ) -> Result<discovery::SettingConfirmation, String> {
        match self {
            Self::Sony(session) => session.apply_setting(connection, cancelled, change),
            Self::OnePlus(session) => session.apply_setting(connection, cancelled, change),
        }
    }
}
