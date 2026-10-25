use flsp::auth::Account;
use flsp::config::{self, LoaderType, Profile};
use flsp::engine::singleplayer::{self, settings::{GameMode, WorldSettings}};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_world_start_join_and_clean_stop() {
    let home = std::env::var("FLSP_HOME").expect("FLSP_HOME");
    std::env::set_var("HOME", &home);
    let engine = std::path::Path::new(&home).join("engine-bin");
    std::fs::create_dir_all(&engine).unwrap();
    let script = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake_mcplay.py")).unwrap();
    std::fs::write(engine.join("mcplay"), script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(engine.join("mcplay"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("FLEI_ENGINE_PATH", engine.join("mcplay"));
    let result_path = std::path::Path::new(&home).join("result.json");
    std::env::set_var("FLSP_RESULT", &result_path);

    let mc = std::env::var("FLSP_MC").unwrap_or_else(|_| "1.20.1".into());
    let profile = Profile { id: "p1".into(), name: "P".into(), minecraft_version: mc.clone(), loader: LoaderType::Fleiengine, ..Profile::default() };
    let account = Account { name: "Steve".into(), uuid: "00000000-0000-0000-0000-000000000001".into(), kind: "offline".into(), xuid: String::new() };
    let entry = singleplayer::create_world("p1", WorldSettings { name: "Test World".into(), seed: "12345".into(), mode: GameMode::Creative, ..WorldSettings::default() }).unwrap();

    let running: Arc<Mutex<HashMap<String, u32>>> = Arc::new(Mutex::new(HashMap::new()));
    let refused = singleplayer::launch_singleplayer(&tauri::Window, &profile, &account, &entry.folder, running.clone()).await;
    assert!(refused.unwrap_err().to_string().contains("EULA"));
    singleplayer::eula::accept().unwrap();
    let started = Instant::now();
    let pid = singleplayer::launch_singleplayer(&tauri::Window, &profile, &account, &entry.folder, running.clone()).await.unwrap();
    println!("client pid {pid} after {:?}", started.elapsed());

    let deadline = Instant::now() + Duration::from_secs(60);
    while running.lock().unwrap().contains_key("p1") && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(!running.lock().unwrap().contains_key("p1"));
    let result: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&result_path).unwrap()).unwrap();
    println!("{result}");
    assert_eq!(result["status_version"].as_str().unwrap(), mc);
    assert_eq!(result["host"], "127.0.0.1");
    assert!(matches!(result["login_packet"].as_u64(), Some(2)), "offline login success expected: {result}");
    let port = result["port"].as_u64().unwrap() as u16;

    let world_dir = singleplayer::profile_worlds_root("p1").join(&entry.folder);
    let level = world_dir.join("world").join("level.dat");
    let deadline = Instant::now() + Duration::from_secs(60);
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(300));
    }
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err(), "server still listening after client exit");
    assert!(level.is_file(), "world was not saved");
    let props = std::fs::read_to_string(world_dir.join("server.properties")).unwrap();
    assert!(props.contains("level-seed=12345") && props.contains("gamemode=creative") && props.contains("online-mode=false"));
    let listed = singleplayer::list_worlds("p1");
    assert!(listed[0].generated);
    let deadline = Instant::now() + Duration::from_secs(60);
    while singleplayer::orphans::read(&world_dir).is_some() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(300));
    }
    assert!(singleplayer::orphans::read(&world_dir).is_none(), "pidfile left behind after clean stop");
    let _ = config::base_dir();

    let orphan = singleplayer::create_world("p1", WorldSettings { name: "Orphan World".into(), ..WorldSettings::default() }).unwrap();
    let orphan_dir = singleplayer::profile_worlds_root("p1").join(&orphan.folder);
    let jar = singleplayer::jar::jar_path(&mc);
    assert!(jar.is_file());
    singleplayer::worlds::prepare_server_files(&orphan_dir, &orphan.settings, singleplayer::port::pick_free_port().unwrap()).unwrap();
    let command = singleplayer::server::build_command(&singleplayer::server::ServerCommand {
        java: "java".into(),
        jar: jar.clone(),
        dir: orphan_dir.clone(),
        ram_mb: 1024,
    });
    let sink: singleplayer::server::LineSink = Arc::new(|_| {});
    let mut server = singleplayer::server::ServerProcess::spawn(command, sink).unwrap();
    server.track(orphan_dir.clone(), &jar);
    server.wait_ready(Duration::from_secs(180)).unwrap();
    let server_pid = server.pid();
    std::mem::forget(server);
    assert!(singleplayer::orphans::is_in_use(&orphan_dir));
    assert!(singleplayer::delete_world("p1", &orphan.folder).is_err(), "delete must refuse a live server");
    let reaped = singleplayer::reap_orphans();
    assert!(reaped.contains(&server_pid), "orphan not reaped: {reaped:?}");
    assert!(singleplayer::orphans::read(&orphan_dir).is_none());
    assert!(singleplayer::probe::command_line(server_pid).is_none(), "orphan still running");
    assert!(orphan_dir.join("world").join("level.dat").is_file(), "orphan world was not saved on SIGTERM");
    singleplayer::delete_world("p1", &orphan.folder).unwrap();
}
