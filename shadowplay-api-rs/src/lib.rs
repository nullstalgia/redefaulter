//! Based on the excellent work and information gathered by @cm-pony for [Experienceless](https://github.com/cm-pony/Experienceless/issues/1)

use errors::ApiResult;
use reqwest::{
    blocking::{Client, ClientBuilder},
    header::HeaderMap,
    IntoUrl, Url,
};
use secret::{SecretContents, SECRET_HEADER};
use structs::{ErrorResponse, MicrophonePresent, ShadowPlayMicrophone};

pub mod errors;
pub use errors::Error;

mod secret;
mod structs;

#[derive(Debug)]
pub struct ShadowPlayActor {
    client: Client,
    secret: SecretContents,
}
impl ShadowPlayActor {
    pub fn build() -> ApiResult<Self> {
        let secret = SecretContents::load()?;

        let headers = {
            let mut map = HeaderMap::new();
            let value = secret.as_header_value()?;
            map.insert(SECRET_HEADER, value);
            map
        };

        let client = ClientBuilder::new().default_headers(headers).build()?;

        Ok(Self { client, secret })
    }
    pub fn reload_secret(&mut self) -> ApiResult<()> {
        self.secret = SecretContents::load()?;
        Ok(())
    }
    fn form_url<U: IntoUrl>(&self, url: U) -> ApiResult<Url> {
        let new_url = format!(
            "http://localhost:{port}/ShadowPlay/v.1.0/{url}",
            port = self.secret.port,
            url = url.as_str()
        );
        Url::parse(&new_url).map_err(|_| Error::UrlForm(url.as_str().to_owned()))
    }
    pub fn microphone_present(&self) -> ApiResult<usize> {
        let url = self.form_url("Microphone/Present")?;
        let resp = self.client.get(url).send()?.text()?;
        let decoded: MicrophonePresent = serde_json::from_str(&resp)?;
        Ok(decoded.present)
    }
    pub fn microphone_get_all(&self) -> ApiResult<Vec<ShadowPlayMicrophone>> {
        let mic_count = self.microphone_present()?;
        let mut mics = Vec::with_capacity(mic_count);
        for index in 0..mic_count {
            let mic = self.microphone_get_index(index)?;
            mics.push(mic);
        }
        Ok(mics)
    }
    pub fn microphone_get_index(&self, index: usize) -> ApiResult<ShadowPlayMicrophone> {
        let url = format!("Microphone/{index}/Settings");
        let url = self.form_url(url)?;
        let resp = self.client.get(url).send()?.text()?;
        let decoded: ShadowPlayMicrophone = serde_json::from_str(&resp)?;
        Ok(decoded)
    }
    pub fn microphone_current(&self) -> ApiResult<ShadowPlayMicrophone> {
        let url = self.form_url("Microphone/Settings")?;
        let resp = self.client.get(url).send()?.text()?;
        let decoded: ShadowPlayMicrophone = serde_json::from_str(&resp)?;
        Ok(decoded)
    }
    pub fn microphone_change(&self, desired_guid: &str) -> ApiResult<()> {
        // First checking current device to see if we can avoid the rest of the operations.
        let current = self.microphone_current()?;
        if current.guid == desired_guid {
            return Ok(());
        }

        let mic_count = self.microphone_present()?;
        for index in 0..mic_count {
            let mic = self.microphone_get_index(index)?;
            if mic.guid == desired_guid {
                let url = format!("Microphone/{index}/Settings", index = mic.index);
                let url = self.form_url(url)?;

                let payload = serde_json::to_string(&mic)?;
                // POST-ing to an index with a body of desired settings selects the device as the one to record from.
                let resp = self.client.post(url).body(payload).send()?;

                if resp.status().is_success() {
                    return Ok(());
                }
                let error_response: Option<ErrorResponse> =
                    serde_json::from_str(&resp.text().unwrap_or_default()).ok();
                return Err(Error::ApiResponse(error_response));
            }
        }

        Err(Error::MicNotFound(desired_guid.to_owned()))
    }
}
