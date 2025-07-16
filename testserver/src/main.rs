use may_minihttp::{HttpServer, HttpService, Request, Response};
use std::io;

#[derive(Clone)]
struct HelloWorld;

impl HttpService for HelloWorld {
    fn call(&mut self, _req: Request, res: &mut Response) -> io::Result<()> {
        res.body("Hello, world!");
        Ok(())
    }
}

fn main() {
    let server = HttpServer(HelloWorld).start("0.0.0.0:8080").unwrap();
    server.join().unwrap();
}
