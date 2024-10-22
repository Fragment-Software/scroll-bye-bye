use std::{collections::HashMap, time::Duration};

use alloy::{
    hex,
    primitives::{Address, FixedBytes, U256},
};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client, Method, Proxy,
};
use serde::Serialize;

use crate::constants::REQUEST_PROOF_URL;

#[derive(Clone)]
pub struct RequestParams<'a, S: Serialize> {
    pub url: &'a str,
    pub method: Method,
    pub body: Option<S>,
    pub query_args: Option<HashMap<&'a str, &'a str>>,
}

pub async fn send_http_request(
    request_params: &RequestParams<'_, impl Serialize>,
    headers: Option<&HeaderMap>,
    proxy: Option<&Proxy>,
) -> eyre::Result<String> {
    let client = proxy.map_or_else(Client::new, |proxy| {
        Client::builder()
            .proxy(proxy.clone())
            .build()
            .unwrap_or_else(|err| {
                tracing::error!("Failed to build a client with proxy: {proxy:?}. Error: {err}");
                Client::new()
            })
    });

    let mut request = client.request(request_params.method.clone(), request_params.url);

    if let Some(params) = &request_params.query_args {
        request = request.query(&params);
    }

    if let Some(body) = &request_params.body {
        request = request.json(&body);
    }

    if let Some(headers) = headers {
        request = request.headers(headers.clone());
    }

    let response = request
        .send()
        .await
        .inspect_err(|e| tracing::error!("Request failed: {}", e))?
        .error_for_status()
        .inspect_err(|e| tracing::error!("Non-successful status code: {}", e))?;

    let text = response
        .text()
        .await
        .inspect_err(|e| tracing::error!("Failed to retrieve response text: {}", e))?;

    Ok(text)
}

pub async fn send_http_request_with_retries(
    request_params: &RequestParams<'_, impl Serialize>,
    headers: Option<&HeaderMap>,
    proxy: Option<&Proxy>,
    max_retries: Option<usize>,
    retry_delay: Option<Duration>,
) -> eyre::Result<String> {
    let max_retries = max_retries.unwrap_or(5);
    let retry_delay = retry_delay.unwrap_or(Duration::from_secs(3));

    for _ in 0..max_retries {
        match send_http_request(request_params, headers, proxy).await {
            Ok(response) => return Ok(response),
            Err(_) => {
                tokio::time::sleep(retry_delay).await;
            }
        }
    }

    eyre::bail!("Amount of tries exceeded")
}

pub async fn get_proof(address: Address, proxy: reqwest::Proxy) -> eyre::Result<String> {
    tracing::info!("Getting proof and allocation for {address}");

    let headers = get_headers();

    let query_args = [("step", "4")]
        .iter() // UNCOMMENT IF GET
        .map(|(arg, value)| (*arg, *value))
        .collect();

    let request_params = RequestParams {
        url: REQUEST_PROOF_URL,
        method: Method::POST,
        body: Some(vec![address.to_string()]),
        query_args: Some(query_args),
    };

    let response =
        send_http_request_with_retries(&request_params, Some(&headers), Some(&proxy), None, None)
            .await?;

    Ok(response)
}

pub fn extract_proof_and_amount(response_text: &str) -> eyre::Result<(Vec<FixedBytes<32>>, U256)> {
    // Split the response text by "1:"
    let parts: Vec<&str> = response_text.splitn(2, "1:").collect();

    if parts.len() < 2 {
        eyre::bail!("Could not find '1:' in the response.");
    }

    let json_str = parts[1].trim();

    let data: serde_json::Value = serde_json::from_str(json_str)?;

    let amount_str = data["amount"]
        .as_str()
        .ok_or_else(|| eyre::eyre!("'amount' field is missing or not a string"))?;
    let amount = U256::from_str_radix(amount_str, 10)?;

    let proof_array = data["proof"]
        .as_array()
        .ok_or_else(|| eyre::eyre!("'proof' field is missing or not an array"))?;
    let proof = proof_array
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>()
        .into_iter()
        .map(|hex_str| {
            let bytes = hex::decode(hex_str).unwrap();
            FixedBytes::from_slice(&bytes)
        })
        .collect();

    Ok((proof, amount))
}

fn get_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();

    // Add headers from the curl command
    headers.insert(
        HeaderName::from_static("accept"),
        HeaderValue::from_static("text/x-component"),
    );
    headers.insert(
        HeaderName::from_static("accept-language"),
        HeaderValue::from_static("en-US,en;q=0.9"),
    );
    headers.insert(
        HeaderName::from_static("cache-control"),
        HeaderValue::from_static("no-cache"),
    );
    headers.insert(
        HeaderName::from_static("content-type"),
        HeaderValue::from_static("text/plain;charset=UTF-8"),
    );
    headers.insert(
        HeaderName::from_static("cookie"),
        HeaderValue::from_static("_ga=GA1.1.1149305761.1729541261; _ga_XR3MGVSHFC=GS1.1.1729558399.3.1.1729558399.0.0.0; _ga_0CM3JHPD29=GS1.1.1729580355.1.1.1729580661.0.0.0; _vcrcs=1.1729581708.3600.NjMzMTc4NTk3MDEyNTg3YTBlYTY1NDhjZjczYjJhYjE=.5d4f132acc2af79b849b411174d86b27"),
    );
    headers.insert(
        HeaderName::from_static("dnt"),
        HeaderValue::from_static("1"),
    );
    headers.insert(
        HeaderName::from_static("next-action"),
        HeaderValue::from_static("2ab5dbb719cdef833b891dc475986d28393ae963"),
    );
    headers.insert(
        HeaderName::from_static("next-router-state-tree"),
        HeaderValue::from_static("%5B%22%22%2C%7B%22children%22%3A%5B%22(claim)%22%2C%7B%22children%22%3A%5B%22__PAGE__%22%2C%7B%7D%2C%22%2F%3Fstep%3D4%22%2C%22refresh%22%5D%7D%5D%7D%2Cnull%2Cnull%2Ctrue%5D"),
    );
    headers.insert(
        HeaderName::from_static("origin"),
        HeaderValue::from_static("https://claim.scroll.io"),
    );
    headers.insert(
        HeaderName::from_static("pragma"),
        HeaderValue::from_static("no-cache"),
    );
    headers.insert(
        HeaderName::from_static("priority"),
        HeaderValue::from_static("u=1, i"),
    );
    headers.insert(
        HeaderName::from_static("referer"),
        HeaderValue::from_static("https://claim.scroll.io/?step=4"),
    );
    headers.insert(
        HeaderName::from_static("sec-ch-ua"),
        HeaderValue::from_static("\"Not?A_Brand\";v=\"99\", \"Chromium\";v=\"130\""),
    );
    headers.insert(
        HeaderName::from_static("sec-ch-ua-mobile"),
        HeaderValue::from_static("?0"),
    );
    headers.insert(
        HeaderName::from_static("sec-ch-ua-platform"),
        HeaderValue::from_static("\"macOS\""),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-dest"),
        HeaderValue::from_static("empty"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-mode"),
        HeaderValue::from_static("cors"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-site"),
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        HeaderName::from_static("user-agent"),
        HeaderValue::from_static("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36"),
    );

    headers
}
