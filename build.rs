use std::fs;
use vergen::{vergen, Config};
use sha2::{Sha224, Digest};

fn static_gen() {
    println!("cargo:rerun-if-changed=static");
    let mut list = Vec::new();

    let mut statics_rs = String::new();
    statics_rs.push_str("//DO NOT MODIFY THIS FILE (statics.rs), it is autogenerated and your changes will be overwritten. See build.rs\n");
    statics_rs.push_str("use rocket::{Route,routes};\n");
    statics_rs.push_str("use rocket::http::ContentType;\n");
    statics_rs.push_str("use rocket::response::{Content,Responder};\n");
    statics_rs.push_str("use super::static_responders::{Tagged,LongLived};\n");

    for path_res in fs::read_dir("./static").unwrap() {
        //let path_res:std::io::Result<std::fs::DirEntry> = path_res;
        let dir_entry = path_res.unwrap();
        let path = dir_entry.path();
        let data = fs::read(&path).unwrap();

        let mut hasher = Sha224::new();
        hasher.update(&data);
        let hash = hex::encode(hasher.finalize());

        let filename:String = path.file_name().unwrap().to_str().unwrap().into();
        println!("cargo:rerun-if-changed=static/{}",filename);
        let hashname = hash.clone() + "-" + &filename;
        let fn_name = filename.replace('.',"_");
        let hash_fn_name = format!("hash_{}", fn_name);
        let ext = filename.split('.').last().unwrap();

        let thing = format!(
            r#"

#[get("/{}")]
fn {}() -> impl Responder<'static> {{
    LongLived(Content(ContentType::from_extension("{}").unwrap(), &include_bytes!("../../static/{}")[..]))
}}

#[get("/{}")]
fn {}() -> impl Responder<'static> {{
    Tagged("{}".into(), Content(ContentType::from_extension("{}").unwrap(), &include_bytes!("../../static/{}")[..]))
}}"#,
            &hashname,
            &hash_fn_name,
            &ext,
            &filename,

            &filename,
            &fn_name,
            &hash,
            &ext,
            &filename,
        );

        statics_rs.push_str(&thing);
        list.push((filename, hashname, fn_name, hash_fn_name));
    }

    statics_rs.push_str("\n\npub fn statics_routes() -> Vec<Route> {\n");
    statics_rs.push_str("    routes![");
    for (_,_,fn_name,hash_fn_name) in &list {
        statics_rs.push_str(fn_name);
        statics_rs.push(',');
        statics_rs.push_str(hash_fn_name);
        statics_rs.push(',');
    }
    statics_rs.push_str("]\n");
    statics_rs.push_str("}\n\n");

    statics_rs.push_str("macro_rules! static_path {\n");
    for (filename, hashname, _, _) in &list {
        statics_rs.push_str(&format!(r#"
    ({}) => {{"/{}"}};
"#, filename, hashname));
    }
    statics_rs.push_str("}\n");
    statics_rs.push_str("\npub(crate) use static_path;\n");

    fs::write("src/web/statics.rs", statics_rs).unwrap();
}

fn main() {
    vergen(Config::default()).unwrap();
    static_gen();
}