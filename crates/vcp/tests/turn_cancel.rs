//! A chat request that gives up while it waits for its turn (its client
//! timed out, the connection closed) must not keep computing waiting for
//! good. (Own test binary: the turn is process-wide.)

use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_chat_request_that_gives_up_doesnt_block_jobs() {
    // VCP works for someone (a design of a computing job)
    let held = ttv_vcp::turn("/api/v1/voice-design/generate").await;
    // chat arrives, waits, and its client gives up
    for _ in 0..3 {
        let waiter = tokio::spawn(async {
            let _t = ttv_vcp::turn("/api/v1/generation/clone").await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
    }
    drop(held);
    // the job's next request gets its turn (after the short pause for chat)
    let t0 = std::time::Instant::now();
    let got = tokio::time::timeout(Duration::from_secs(5), ttv_vcp::background_turn("/api/v1/voices")).await;
    assert!(got.is_ok(), "computing still waits for chat requests that gave up");
    // even after the very first chat turn of this run
    assert!(t0.elapsed() >= Duration::from_millis(1400), "no pause after chat: {:?}", t0.elapsed());
    drop(got);
    // and chat still goes first when it really waits
    let job = ttv_vcp::background_turn("/api/v1/voices").await;
    let chat = tokio::spawn(async { drop(ttv_vcp::turn("/api/v1/generation/clone").await) });
    tokio::time::sleep(Duration::from_millis(50)).await;
    drop(job);
    tokio::time::timeout(Duration::from_secs(2), chat).await.expect("chat got its turn").unwrap();
}
