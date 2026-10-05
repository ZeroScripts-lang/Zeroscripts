use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::heap::HeapObject;

use super::helpers::{duration_ms, error_chain, expect_string_arg};
use super::{HttpOutcome, HttpSlot, TaskKind, TaskState, Value, VmError, ZeroVM};

impl ZeroVM {
    /// `http.get/post/put/patch/delete` (§50, §50.1, §59, §60).
    ///
    /// Validates options synchronously (wrong types → ERROR per §59/§60),
    /// spawns a worker thread for the blocking exchange, and returns a Task
    /// the caller awaits (§101). The task is `running` from creation: the
    /// request is already in flight (§106).
    pub(super) fn http_request(&mut self, method: &str, args: &[Value], fname: &str) -> Result<Value, VmError> {
        if args.is_empty() || args.len() > 2 {
            return Err(VmError::RuntimeError(format!(
                "{} expects 1 or 2 arguments",
                fname
            )));
        }
        let url = expect_string_arg(args, 0, fname)?.to_string();
        let mut query_pairs: Vec<String> = Vec::new();
        let mut headers: Vec<(String, String)> = Vec::new();
        let mut body: Option<String> = None;
        let mut timeout_ms: Option<u64> = None;

        if let Some(opt) = args.get(1) {
            let obj_ref = match opt {
                Value::Object(r) => r.clone(),
                _ => {
                    return Err(VmError::RuntimeError(format!(
                        "{} options must be an object",
                        fname
                    )))
                }
            };
            let props = match self.heap.get(&obj_ref) {
                Ok(HeapObject::Object { props, .. }) => props.clone(),
                _ => {
                    return Err(VmError::RuntimeError(format!(
                        "{} options must be an object",
                        fname
                    )))
                }
            };
            for (key, vr) in props {
                let v = Value::from_heap_ref(&vr, &self.heap)
                    .map_err(|_| VmError::RuntimeError("Failed to read options".to_string()))?;
                match key.as_str() {
                    // §59: object; string keys; string/number/boolean/null
                    // values; everything URL-encoded; null → empty value.
                    "query" => match v {
                        Value::Object(qr) => {
                            let qprops = match self.heap.get(&qr) {
                                Ok(HeapObject::Object { props, .. }) => props.clone(),
                                _ => {
                                    return Err(VmError::RuntimeError(format!(
                                        "{} query must be an object",
                                        fname
                                    )))
                                }
                            };
                            for (qk, qvr) in qprops {
                                let qv = Value::from_heap_ref(&qvr, &self.heap).map_err(|_| {
                                    VmError::RuntimeError("Failed to read query".to_string())
                                })?;
                                let encoded = match &qv {
                                    Value::Str(s) => percent_encode_component(s),
                                    Value::Number(n) => {
                                        percent_encode_component(&format!("{}", n))
                                    }
                                    Value::Boolean(b) => {
                                        percent_encode_component(if *b { "true" } else { "false" })
                                    }
                                    Value::Null => String::new(),
                                    _ => {
                                        return Err(VmError::RuntimeError(format!(
                                            "{} query values must be string, number, boolean, or null",
                                            fname
                                        )))
                                    }
                                };
                                query_pairs
                                    .push(format!("{}={}", percent_encode_component(&qk), encoded));
                            }
                        }
                        _ => {
                            return Err(VmError::RuntimeError(format!(
                                "{} query must be an object",
                                fname
                            )))
                        }
                    },
                    // §60: object; string keys; values MUST be strings.
                    "headers" => match v {
                        Value::Object(hr) => {
                            let hprops = match self.heap.get(&hr) {
                                Ok(HeapObject::Object { props, .. }) => props.clone(),
                                _ => {
                                    return Err(VmError::RuntimeError(format!(
                                        "{} headers must be an object",
                                        fname
                                    )))
                                }
                            };
                            for (hk, hvr) in hprops {
                                let hv = Value::from_heap_ref(&hvr, &self.heap).map_err(|_| {
                                    VmError::RuntimeError("Failed to read headers".to_string())
                                })?;
                                match hv {
                                    Value::Str(s) => headers.push((hk, s)),
                                    _ => {
                                        return Err(VmError::RuntimeError(format!(
                                            "{} header values must be strings",
                                            fname
                                        )))
                                    }
                                }
                            }
                        }
                        _ => {
                            return Err(VmError::RuntimeError(format!(
                                "{} headers must be an object",
                                fname
                            )))
                        }
                    },
                    // §50.1 shows body as a string (json.stringify output);
                    // the spec gives no other body form → string only.
                    "body" => match v {
                        Value::Str(s) => body = Some(s),
                        _ => {
                            return Err(VmError::RuntimeError(format!(
                                "{} body must be a string",
                                fname
                            )))
                        }
                    },
                    // §50.1: timeout in milliseconds, non-negative integer.
                    "timeout" => {
                        let ms = duration_ms(&v, fname)?;
                        timeout_ms = Some(u64::try_from(ms).unwrap_or(u64::MAX));
                    }
                    // Unknown option keys are ignored (spec is silent).
                    _ => {}
                }
            }
        }

        let final_url = append_query(&url, &query_pairs);
        let slot: HttpSlot = Arc::new(Mutex::new(None));
        let worker_slot = slot.clone();
        let worker_method = method.to_string();
        let worker_fname = fname.to_string();
        thread::spawn(move || {
            let result = http_perform(
                &worker_method,
                &final_url,
                &headers,
                body.as_deref(),
                timeout_ms,
            )
            .map_err(|e| format!("{}: {}", worker_fname, e));
            if let Ok(mut guard) = worker_slot.lock() {
                *guard = Some(result);
            }
        });

        let id = self.create_synthetic_task(TaskKind::Http { slot })?;
        // The exchange started at creation (§106 "running": sedang
        // menjalankan operasi asynchronous).
        self.tasks[id].state = TaskState::Running;
        Ok(Value::Task(id))
    }

    /// Convert a finished exchange into the §50 response object:
    /// `{ status: number, headers: object, body: string }`. The headers
    /// object is an independent copy — mutating it cannot touch the
    /// response or any future request (§60).
    pub(super) fn build_http_response(&mut self, out: &HttpOutcome) -> Result<Value, VmError> {
        let mut hprops = Vec::with_capacity(out.headers.len());
        for (k, v) in &out.headers {
            let hr = Value::Str(v.clone()).heap_object_for_element(&mut self.heap);
            hprops.push((k.clone(), hr));
        }
        let headers_ref = self.heap.allocate(HeapObject::object(hprops));

        let fields = [
            ("status", Value::Number(out.status as f64)),
            ("headers", Value::Object(headers_ref)),
            ("body", Value::Str(out.body.clone())),
        ];
        let mut props = Vec::with_capacity(fields.len());
        for (name, val) in fields {
            let r = val.heap_object_for_element(&mut self.heap);
            props.push((name.to_string(), r));
        }
        let obj = self.heap.allocate(HeapObject::object(props));
        self.gc.on_allocation();
        Ok(Value::Object(obj))
    }

    /// Finish every HTTP task whose worker result has landed. Returns true
    /// if any task transitioned (scheduler should loop again).
    pub(super) fn poll_http_completed(&mut self) -> Result<bool, VmError> {
        for tid in 0..self.tasks.len() {
            if self.tasks[tid].state.is_terminal() {
                continue;
            }
            let slot = match &self.tasks[tid].kind {
                TaskKind::Http { slot } => slot.clone(),
                _ => continue,
            };
            let taken = match slot.lock() {
                Ok(mut guard) => guard.take(),
                Err(_) => None,
            };
            if let Some(res) = taken {
                match res {
                    Ok(outcome) => {
                        let value = self.build_http_response(&outcome)?;
                        self.finish_task(tid, &value)?;
                    }
                    Err(msg) => self.fail_task(tid, msg)?,
                }
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// True while at least one HTTP exchange is still on the wire. The
    /// scheduler parks in real time for these — virtual time must not jump
    /// past HTTP work, which progresses on the real clock (ISSUES §65).
    pub(super) fn http_inflight(&self) -> bool {
        self.tasks
            .iter()
            .any(|t| !t.state.is_terminal() && matches!(t.kind, TaskKind::Http { .. }))
    }

}

/// Percent-encode one query component (§59: everything URL-encoded;
/// RFC 3986 unreserved characters are kept readable).
fn percent_encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Append encoded query pairs to a URL (§59): a URL that already has a
/// query keeps it (appended with `&`, not replaced); fragments stay last.
fn append_query(url: &str, pairs: &[String]) -> String {
    if pairs.is_empty() {
        return url.to_string();
    }
    let (base, frag) = match url.find('#') {
        Some(i) => (&url[..i], &url[i..]),
        None => (url, ""),
    };
    let sep = if base.contains('?') {
        if base.ends_with('?') || base.ends_with('&') {
            ""
        } else {
            "&"
        }
    } else {
        "?"
    };
    format!("{}{}{}{}", base, sep, pairs.join("&"), frag)
}


/// Perform one blocking HTTP exchange on a worker thread (§50).
///
/// - 4xx/5xx are ordinary responses (`http_status_as_error(false)` per §50.1)
/// - `timeout` (§50.1) bounds the whole operation in real time
/// - response bodies are always read as (lossy UTF-8) strings — no JSON
///   parsing at the HTTP layer (§50, §51)
fn http_perform(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&str>,
    timeout_ms: Option<u64>,
) -> Result<HttpOutcome, String> {
    let timeout = timeout_ms.map(Duration::from_millis);

    // Build + run a request, applying user headers and §50.1 options.
    // GET/DELETE builders have no body unless force_send_body() is used.
    let resp = match method {
        "GET" => {
            let mut rb = ureq::get(url);
            for (k, v) in headers {
                rb = rb.header(k.as_str(), v.as_str());
            }
            let rb = rb
                .config()
                .http_status_as_error(false)
                .timeout_global(timeout)
                .build();
            match body {
                Some(b) => rb.force_send_body().send(b),
                None => rb.call(),
            }
        }
        "DELETE" => {
            let mut rb = ureq::delete(url);
            for (k, v) in headers {
                rb = rb.header(k.as_str(), v.as_str());
            }
            let rb = rb
                .config()
                .http_status_as_error(false)
                .timeout_global(timeout)
                .build();
            match body {
                Some(b) => rb.force_send_body().send(b),
                None => rb.call(),
            }
        }
        "POST" => {
            let mut rb = ureq::post(url);
            for (k, v) in headers {
                rb = rb.header(k.as_str(), v.as_str());
            }
            let rb = rb
                .config()
                .http_status_as_error(false)
                .timeout_global(timeout)
                .build();
            match body {
                Some(b) => rb.send(b),
                None => rb.send_empty(),
            }
        }
        "PUT" => {
            let mut rb = ureq::put(url);
            for (k, v) in headers {
                rb = rb.header(k.as_str(), v.as_str());
            }
            let rb = rb
                .config()
                .http_status_as_error(false)
                .timeout_global(timeout)
                .build();
            match body {
                Some(b) => rb.send(b),
                None => rb.send_empty(),
            }
        }
        "PATCH" => {
            let mut rb = ureq::patch(url);
            for (k, v) in headers {
                rb = rb.header(k.as_str(), v.as_str());
            }
            let rb = rb
                .config()
                .http_status_as_error(false)
                .timeout_global(timeout)
                .build();
            match body {
                Some(b) => rb.send(b),
                None => rb.send_empty(),
            }
        }
        other => return Err(format!("unsupported HTTP method '{}'", other)),
    };

    let mut resp = resp.map_err(|e| match &e {
        ureq::Error::Timeout(_) => match timeout_ms {
            Some(ms) => format!("request timed out after {} ms", ms),
            None => "request timed out".to_string(),
        },
        _ => error_chain(&e),
    })?;

    let status = resp.status().as_u16();
    let mut out_headers = Vec::with_capacity(resp.headers().len());
    for (name, value) in resp.headers().iter() {
        out_headers.push((
            name.as_str().to_string(),
            String::from_utf8_lossy(value.as_bytes()).into_owned(),
        ));
    }
    let resp_body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("failed to read response body: {}", error_chain(&e)))?;
    Ok(HttpOutcome {
        status,
        headers: out_headers,
        body: resp_body,
    })
}
