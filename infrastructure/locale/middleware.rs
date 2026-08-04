use axum::{
    extract::Request,
    http::header,
    middleware::Next,
    response::{IntoResponse as _, Response},
};
use axum_extra::extract::CookieJar;
use trace_kit::extract_trace_id;
use web::error::WebError;
use web::response::json_response::ProblemDetails;

use crate::{DEFAULT_LOCALE, translate};

const LANGUAGE_COOKIE_NAME: &str = "accept_language";

pub async fn l10n_middleware(request: Request, next: Next) -> Response {
    let instance = request.uri().path().to_string();
    let trace_id = extract_trace_id(request.headers());
    let locale = parse_locale(&request);
    let response = next.run(request).await;
    if let Some(err) = response.extensions().get::<WebError>() {
        let info = translate(&locale, &err.to_string());
        let problem = ProblemDetails::new(
            err.problem_type(),
            err.problem_title(),
            err.status_code(),
            Some(info),
            Some(instance),
            Some(err.error_code().to_string()),
            trace_id,
        );
        return problem.into_response();
    }
    response
}

fn parse_locale(request: &Request) -> String {
    let cookie_lang = CookieJar::from_headers(request.headers())
        .get(LANGUAGE_COOKIE_NAME)
        .map(|cookie| cookie.value().to_owned());

    let header_lang = request
        .headers()
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| accept_language::parse(s).first().cloned());

    cookie_lang
        .or(header_lang)
        .map(|lang| {
            let primary = lang.as_str().split('-').next().unwrap_or("");
            match primary {
                "zh" => "zh-CN",
                "en" => "en-US",
                _ => DEFAULT_LOCALE,
            }
            .to_string()
        })
        .unwrap_or_else(|| DEFAULT_LOCALE.to_string())
}
