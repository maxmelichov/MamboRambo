use tauri::State;

use crate::{analytics, runner::errors::track_runner_err};

use super::{
    dto::{LanguagesResponse, LoadModelRequest, SpeechRequest, VoicesResponse},
    errors::{get_json, json_response, response_error},
    process::RunnerState,
    runner_client,
};

pub async fn load_model_request(
    app: tauri::AppHandle,
    state: State<'_, RunnerState>,
    request: LoadModelRequest,
) -> Result<serde_json::Value, String> {
    let (client, base_url) = runner_client(&app, &state)?;
    let runtime = "blue";
    let body = serde_json::json!({
        "model_path": request.model_path,
        "renikud_path": request.renikud_path,
    });

    let response = client
        .post(format!("{base_url}/v1/models/load"))
        .json(&body)
        .send()
        .await
        .map_err(|err| {
            track_runner_err(
                &app,
                analytics::events::ERROR_MODEL_LOAD_FAILED,
                format!("failed to send model load request: {err}"),
                "load_model",
                &runtime,
            )
        })?;
    json_response(response).await.map_err(|err| {
        track_runner_err(
            &app,
            analytics::events::ERROR_MODEL_LOAD_FAILED,
            err,
            "load_model",
            &runtime,
        )
    })
}

pub async fn get_languages_request(
    app: tauri::AppHandle,
    state: State<'_, RunnerState>,
) -> Result<Vec<String>, String> {
    let (client, base_url) = runner_client(&app, &state)?;
    let body = get_json::<LanguagesResponse>(
        &app,
        &client,
        &format!("{base_url}/v1/languages"),
        "get_languages",
        "languages",
    )
    .await?;
    Ok(body.languages)
}

pub async fn get_voices_request(
    app: tauri::AppHandle,
    state: State<'_, RunnerState>,
) -> Result<Vec<String>, String> {
    let (client, base_url) = runner_client(&app, &state)?;
    let body = get_json::<VoicesResponse>(
        &app,
        &client,
        &format!("{base_url}/v1/voices"),
        "get_voices",
        "voices",
    )
    .await?;
    Ok(body.voices)
}

pub async fn synthesize_request(
    app: tauri::AppHandle,
    state: State<'_, RunnerState>,
    request: SpeechRequest,
) -> Result<String, String> {
    let (client, base_url) = runner_client(&app, &state)?;
    let output_path = request
        .output_path
        .unwrap_or_else(super::default_output_path);
    let language = request.language.unwrap_or_else(|| "auto".to_string());
    let voice = request.voice.unwrap_or_default();
    let has_voice_reference = request
        .voice_reference
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    let body = serde_json::json!({
        "input": request.input,
        "voice_reference": request.voice_reference.unwrap_or_default(),
        "voice": voice,
        "response_format": "wav",
        "language": language,
    });
    let props = || {
        serde_json::json!({
            "operation": "synthesize",
            "voice": body["voice"].as_str().unwrap_or_default(),
            "language": body["language"].as_str().unwrap_or("auto"),
            "has_voice_reference": has_voice_reference,
        })
    };

    let response = client
        .post(format!("{base_url}/v1/audio/speech"))
        .json(&body)
        .send()
        .await
        .map_err(|err| {
            analytics::track_error(
                &app,
                analytics::events::ERROR_SYNTHESIS_FAILED,
                format!("failed to send speech request: {err}"),
                props(),
            )
        })?;
    if !response.status().is_success() {
        let err = response_error(response).await;
        return Err(analytics::track_error(
            &app,
            analytics::events::ERROR_SYNTHESIS_FAILED,
            err,
            props(),
        ));
    }

    let bytes = response.bytes().await.map_err(|err| {
        analytics::track_error(
            &app,
            analytics::events::ERROR_SYNTHESIS_FAILED,
            format!("failed to read speech response: {err}"),
            props(),
        )
    })?;
    tauri::async_runtime::spawn_blocking({
        let output_path = output_path.clone();
        move || {
            std::fs::write(&output_path, bytes)
                .map_err(|err| format!("failed to write {output_path}: {err}"))
        }
    })
    .await
    .map_err(|err| {
        analytics::track_error(
            &app,
            analytics::events::ERROR_SYNTHESIS_FAILED,
            format!("failed to join file write task: {err}"),
            props(),
        )
    })?
    .map_err(|err| {
        analytics::track_error(
            &app,
            analytics::events::ERROR_SYNTHESIS_FAILED,
            err,
            props(),
        )
    })?;
    analytics::track_event_handle_with_props(
        &app,
        analytics::events::SYNTHESIS_COMPLETED,
        Some(props()),
    );
    Ok(output_path)
}
