use crate::common::{relay_bin, PortReservation, World};
use std::{
    fs::File,
    process::{Child, Command, Stdio},
    time::Duration,
};

pub struct Sites {
    child: Child,
    _ports: Vec<PortReservation>,
    pub public: String,
    pub management: String,
    upstream: u16,
    proxy: u16,
}
impl Drop for Sites {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Sites {
    pub fn reserve() -> (Vec<PortReservation>, String, String) {
        let ports: Vec<_> = (0..3).map(|_| PortReservation::acquire(None)).collect();
        let management = format!("http://127.0.0.1:{}", ports[0].port());
        let public = format!("http://localhost:{}", ports[1].port());
        (ports, management, public)
    }
    pub async fn start(
        w: &World,
        ports: Vec<PortReservation>,
        management: String,
        public: String,
    ) -> Self {
        let upstream = ports[2].port();
        let proxy = ports[0].port();
        let log = File::create(w.dir.join("sites.log")).unwrap();
        let child = Command::new(
            relay_bin().with_file_name(format!("zoen-sites{}", std::env::consts::EXE_SUFFIX)),
        )
        .env("DATABASE_URL", &w.db_url)
        .env("ZOEN_RELAY_NAME", w.relay_name())
        .env("ZOEN_SITES_MANAGEMENT_ORIGIN", &management)
        .env("ZOEN_SITES_PUBLIC_ORIGIN", &public)
        .env(
            "ZOEN_SITES_MANAGEMENT_BIND",
            format!("127.0.0.1:{upstream}"),
        )
        .env(
            "ZOEN_SITES_PUBLIC_BIND",
            format!("127.0.0.1:{}", ports[1].port()),
        )
        .env("ZOEN_SITES_BLOB_DIR", w.dir.join("public-sites"))
        .env_remove("ZOEN_S3_BUCKET")
        .env_remove("ZOEN_S3_ENDPOINT")
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .unwrap();
        let mut service = Self {
            child,
            _ports: ports,
            public,
            management,
            upstream,
            proxy,
        };
        for _ in 0..100 {
            assert!(
                service.child.try_wait().unwrap().is_none(),
                "Sites exited; see {}",
                w.dir.join("sites.log").display()
            );
            if tokio::net::TcpStream::connect(("127.0.0.1", upstream))
                .await
                .is_ok()
                && tokio::net::TcpStream::connect(("127.0.0.1", service._ports[1].port()))
                    .await
                    .is_ok()
            {
                return service;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("Sites did not bind owned ports");
    }
    pub async fn proxy(&self) -> super::proxy::ReplyLoss {
        super::proxy::ReplyLoss::start(self.proxy, self.upstream).await
    }
}
