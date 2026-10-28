use zeebx::handset::audio::pcm_queue;

#[test]
fn reconnect_at_a_new_native_rate_preserves_remaining_voice_duration() {
    let mixer = zeebx::audio::Mixer::silent(8_000);
    mixer.play(
        1,
        std::sync::Arc::new(zeebx::audio::wav::Sound {
            rate: 8_000,
            channels: 1,
            samples: vec![0.25; 8_000],
        }),
        1.0,
        1,
    );
    mixer.render(2_000);
    mixer.define_taxa(16_000);
    mixer.render(11_900);
    assert!(mixer.is_playing(1));
    mixer.render(200);
    assert!(!mixer.is_playing(1));
}

#[test]
fn oboe_stereo_tuples_are_filled_without_assuming_rust_tuple_layout() {
    let (mut producer, mut consumer, monitor) = pcm_queue(4);
    producer.push(&[0.5, -0.25], monitor.generation());
    let mut output = [(9.0, 9.0); 2];
    consumer.fill_stereo(&mut output);
    assert_eq!(output, [(0.5, -0.25), (0.0, 0.0)]);
    assert_eq!(monitor.snapshot().nonzero_samples, 2);
}

#[test]
fn asset_worker_preserves_identity_and_rejects_oversized_requests() {
    use zeebx::audio::decode::{DecodeSettings, DecodeWorker};
    let mut worker = DecodeWorker::new(DecodeSettings::default()).unwrap();
    let bytes = zeebx::audio::to_wav(&[0.25; 320], 8000);
    worker.submit(42, bytes).unwrap();
    assert!(worker.submit(99, vec![0; 4 * 1024 * 1024 + 1]).is_err());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let result = loop {
        if let Some(result) = worker.try_recv() {
            break result;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "decoder não devolveu a carga"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert_eq!(result.key, 42);
    let sound = result.sound.unwrap();
    assert_eq!((sound.rate, sound.channels, sound.frames()), (8000, 2, 160));
    assert!(sound.samples.iter().all(|v| (*v - 0.25).abs() < 0.001));
}

#[test]
fn asset_worker_keeps_a_bounded_burst_until_its_channel_can_accept_it() {
    use zeebx::audio::decode::{DecodeSettings, DecodeWorker};
    let mut worker = DecodeWorker::new(DecodeSettings::default()).unwrap();
    for key in 1..=48 {
        worker
            .submit(key, zeebx::audio::to_wav(&[0.25; 320], 8000))
            .unwrap();
    }
    let mut keys = std::collections::BTreeSet::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while keys.len() < 48 {
        if let Some(result) = worker.try_recv() {
            assert!(result.sound.is_ok());
            keys.insert(result.key);
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(
        keys.into_iter().collect::<Vec<_>>(),
        (1..=48).collect::<Vec<_>>()
    );
}

#[test]
fn worker_mixes_pcm_while_the_native_callback_only_consumes_it() {
    let mixer = zeebx::audio::Mixer::silent(48_000);
    mixer.play(
        1,
        std::sync::Arc::new(zeebx::audio::wav::Sound {
            rate: 48_000,
            channels: 1,
            samples: vec![0.25; 512],
        }),
        1.0,
        0,
    );
    let (pump, mut consumer) = zeebx::audio::queued::AudioPump::start(mixer, 128).unwrap();
    let monitor = pump.monitor();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while monitor.snapshot().queued_frames < 64 {
        assert!(
            std::time::Instant::now() < deadline,
            "PCM worker did not fill the queue"
        );
        std::thread::yield_now();
    }
    let mut output = [0.0; 128];
    consumer.fill(&mut output, 2);
    assert!(output.iter().all(|sample| *sample == 0.25));
    assert_eq!(monitor.snapshot().underrun_frames, 0);
    pump.set_paused(true);
    consumer.fill(&mut output, 2);
    assert!(output.iter().all(|sample| *sample == 0.0));
    drop(pump);
    assert!(monitor.snapshot().disconnected);
    consumer.fill(&mut output, 2);
    assert!(output.iter().all(|sample| *sample == 0.0));
}

#[test]
fn pcm_queue_is_bounded_and_preserves_stereo_across_wraparound() {
    let (mut producer, mut consumer, monitor) = pcm_queue(3);
    let epoch = monitor.generation();
    assert_eq!(
        producer.push(&[0.1, -0.1, 0.2, -0.2, 0.3, -0.3, 0.4, -0.4], epoch),
        3
    );
    let mut first = [9.0; 4];
    consumer.fill(&mut first, 2);
    assert_eq!(first, [0.1, -0.1, 0.2, -0.2]);
    assert_eq!(producer.push(&[0.4, -0.4, 0.5, -0.5], epoch), 2);
    let mut next = [9.0; 6];
    consumer.fill(&mut next, 2);
    assert_eq!(next, [0.3, -0.3, 0.4, -0.4, 0.5, -0.5]);
    assert_eq!(monitor.snapshot().underrun_frames, 0);
}

#[test]
fn empty_pcm_queue_outputs_silence_and_counts_missing_frames() {
    let (_, mut consumer, monitor) = pcm_queue(4);
    let mut out = [9.0; 6];
    consumer.fill(&mut out, 2);
    assert_eq!(out, [0.0; 6]);
    let stats = monitor.snapshot();
    assert_eq!(stats.underrun_frames, 3);
    assert_eq!(stats.underrun_callbacks, 1);
    assert_eq!(stats.requested_frames, 3);
    assert_eq!(stats.nonzero_samples, 0);
}

#[test]
fn pause_resume_discards_queued_and_inflight_audio_from_the_old_epoch() {
    let (mut producer, mut consumer, monitor) = pcm_queue(8);
    let before = monitor.generation();
    producer.push(&[0.5, 0.5], before);
    monitor.set_paused(true);
    let mut out = [9.0; 2];
    consumer.fill(&mut out, 2);
    assert_eq!(out, [0.0; 2]);
    assert_eq!(monitor.snapshot().underrun_frames, 0);
    monitor.set_paused(false);
    // Um bloco que terminou de misturar durante a pausa não pode vazar na retomada.
    producer.push(&[0.6, 0.6], before);
    producer.push(&[0.25, -0.25], monitor.generation());
    consumer.fill(&mut out, 2);
    assert_eq!(out, [0.25, -0.25]);
    assert_eq!(monitor.snapshot().discarded_frames, 2);
}

#[test]
fn disconnected_callback_stays_silent_without_consuming_guest_state() {
    let (mut producer, mut consumer, monitor) = pcm_queue(8);
    producer.push(&[0.5, -0.5], monitor.generation());
    monitor.disconnect();
    let mut out = [9.0; 2];
    consumer.fill(&mut out, 2);
    assert_eq!(out, [0.0; 2]);
    assert!(monitor.snapshot().disconnected);
    assert_eq!(monitor.snapshot().underrun_callbacks, 0);
}

#[test]
fn pcm_queue_remains_in_order_with_concurrent_producer_and_callback() {
    let (mut producer, mut consumer, monitor) = pcm_queue(31);
    let epoch = monitor.generation();
    let worker = std::thread::spawn(move || {
        for index in 1..=20_000 {
            let sample = index as f32 / 32_768.0;
            while producer.push(&[sample, -sample], epoch) == 0 {
                std::thread::yield_now();
            }
        }
    });
    let mut expected = 1;
    while expected <= 20_000 {
        let mut out = [0.0; 14];
        consumer.fill(&mut out, 2);
        for frame in out.chunks_exact(2) {
            if frame[0] == 0.0 {
                continue;
            }
            assert_eq!(
                frame,
                [expected as f32 / 32_768.0, -(expected as f32 / 32_768.0)]
            );
            expected += 1;
        }
    }
    worker.join().unwrap();
    assert_eq!(monitor.snapshot().consumed_frames, 20_000);
}
