//! Frame profiling, compiled only with `--features profile`.
//!
//! Serves live profile data for `puffin_viewer` on 127.0.0.1:8585. When
//! `HESTIA_PROFILE_DUMP=<path>` is set, additionally rewrites an aggregated
//! per-scope text report at that path every few seconds, so a capture survives
//! killing the app and needs no viewer.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Duration;

pub fn init() {
    puffin::set_scopes_on(true);
    match puffin_http::Server::new("127.0.0.1:8585") {
        Ok(server) => {
            tracing::info!("puffin server listening on 127.0.0.1:8585");
            // Serve for the lifetime of the process.
            std::mem::forget(server);
        }
        Err(err) => tracing::warn!("failed to start puffin server: {err}"),
    }

    if let Some(path) = std::env::var_os("HESTIA_PROFILE_DUMP") {
        let view = puffin::GlobalFrameView::default();
        // Packing saves RAM for long captures but re-unpacking every dump would
        // burn CPU inside the very frames being measured.
        view.lock().set_pack_frames(false);
        let path = PathBuf::from(path);
        std::thread::Builder::new()
            .name("hestia-profile-dump".into())
            .spawn(move || dump_loop(&view, &path))
            .expect("failed to spawn profile dump thread");
    }
}

fn dump_loop(view: &puffin::GlobalFrameView, path: &PathBuf) {
    loop {
        std::thread::sleep(Duration::from_secs(5));
        let report = build_report(&view.lock());
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, report).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

#[derive(Default)]
struct ScopeAgg {
    count: u64,
    total_ns: i64,
    self_ns: i64,
    max_ns: i64,
}

fn build_report(frames: &puffin::FrameView) -> String {
    let scope_names = frames.scope_collection();
    let mut agg: HashMap<(String, String), ScopeAgg> = HashMap::new();
    let mut frame_durations_ns: Vec<i64> = Vec::new();

    for frame in frames.recent_frames() {
        let Ok(unpacked) = frame.unpacked() else {
            continue;
        };
        frame_durations_ns.push(frame.duration_ns());
        for (thread_info, stream_info) in &unpacked.thread_streams {
            let stream = &stream_info.stream;
            accumulate(
                stream,
                puffin::Reader::from_start(stream),
                scope_names,
                &thread_info.name,
                &mut agg,
            );
        }
    }

    let frame_count = frame_durations_ns.len();
    let mut report = String::new();
    let _ = writeln!(report, "# Hestia frame profile (rolling window)");
    let _ = writeln!(report, "frames: {frame_count}");
    if frame_count == 0 {
        return report;
    }

    frame_durations_ns.sort_unstable();
    let mean = frame_durations_ns.iter().sum::<i64>() / frame_count as i64;
    let pct = |p: usize| frame_durations_ns[(frame_count - 1) * p / 100];
    let _ = writeln!(
        report,
        "frame wall time: mean {:.3} ms, p50 {:.3} ms, p95 {:.3} ms, max {:.3} ms",
        mean as f64 / 1e6,
        pct(50) as f64 / 1e6,
        pct(95) as f64 / 1e6,
        pct(100) as f64 / 1e6,
    );
    let _ = writeln!(report);
    let _ = writeln!(
        report,
        "{:<58} {:>9} {:>12} {:>13} {:>10}",
        "scope (thread)", "calls/frm", "self us/frm", "total us/frm", "max ms"
    );

    let mut rows: Vec<((String, String), ScopeAgg)> = agg.into_iter().collect();
    rows.sort_by_key(|(_, a)| -a.self_ns);
    for ((thread, name), a) in rows.iter().take(60) {
        let _ = writeln!(
            report,
            "{:<58} {:>9.2} {:>12.1} {:>13.1} {:>10.3}",
            format!("{name} ({thread})"),
            a.count as f64 / frame_count as f64,
            a.self_ns as f64 / frame_count as f64 / 1e3,
            a.total_ns as f64 / frame_count as f64 / 1e3,
            a.max_ns as f64 / 1e6,
        );
    }
    report
}

/// Walks scopes recursively, accumulating inclusive and self time per scope
/// name. Returns the summed inclusive time of the scopes at this level.
fn accumulate(
    stream: &puffin::Stream,
    reader: puffin::Reader<'_>,
    scope_names: &puffin::ScopeCollection,
    thread: &str,
    agg: &mut HashMap<(String, String), ScopeAgg>,
) -> i64 {
    let mut level_total_ns = 0;
    for scope in reader.flatten() {
        let inclusive_ns = scope.record.duration_ns;
        level_total_ns += inclusive_ns;
        let children_ns = match puffin::Reader::with_offset(stream, scope.child_begin_position) {
            Ok(child_reader) => accumulate(stream, child_reader, scope_names, thread, agg),
            Err(_) => 0,
        };
        let name = scope_names
            .fetch_by_id(&scope.id)
            .map(|details| match &details.scope_name {
                Some(scope_name) => scope_name.to_string(),
                None => details.function_name.to_string(),
            })
            .unwrap_or_else(|| format!("{:?}", scope.id));
        let entry = agg.entry((thread.to_owned(), name)).or_default();
        entry.count += 1;
        entry.total_ns += inclusive_ns;
        entry.self_ns += inclusive_ns - children_ns;
        entry.max_ns = entry.max_ns.max(inclusive_ns);
    }
    level_total_ns
}
