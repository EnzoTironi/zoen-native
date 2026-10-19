//! Public synthetic RSA assertions for native/Node/workerd policy equivalence.
//! The disposable signing key is never configured with Apple or Google.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine as _};
use ring::{
    rand::SystemRandom,
    signature::{RsaKeyPair, RSA_PKCS1_SHA256},
};
use roda_relay_admission::auth::verify_json;
use serde_json::{json, Value};

fn signed(header: Value, claims: Value) -> String {
    let key =
        RsaKeyPair::from_pkcs8(include_bytes!("../src/auth/fixtures/rsa-test-only.pk8")).unwrap();
    let input = format!(
        "{}.{}",
        B64.encode(serde_json::to_vec(&header).unwrap()),
        B64.encode(serde_json::to_vec(&claims).unwrap())
    );
    let mut signature = vec![0; key.public().modulus_len()];
    key.sign(
        &RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        input.as_bytes(),
        &mut signature,
    )
    .unwrap();
    format!("{input}.{}", B64.encode(signature))
}
fn claims() -> Value {
    json!({"iss":"accounts.google.com","sub":"synthetic-subject","aud":"web-client",
        "iat":1_700_000_000,"exp":1_700_000_600,"nonce":"aa".repeat(32),"name":"Display hint",
        "email":"never-an-account-key@example.invalid"})
}
fn context() -> Value {
    json!({"provider":"google","audience":"web-client","presenter":null,"nonce":"aa".repeat(32),"created_ms":1_700_000_000_000i64})
}
fn main() {
    let header = json!({"alg":"RS256","kid":"test-key","typ":"JWT"});
    let public_key: Value =
        serde_json::from_str(include_str!("../src/auth/fixtures/rsa-test-only.jwk.json")).unwrap();
    let mut cases = Vec::new();
    let mut add = |name: &str, token: String, key: Value, context: Value, positive: bool| {
        let now_ms = "1700000000000";
        let result = verify_json(&token, &key.to_string(), &context.to_string(), now_ms);
        assert_eq!(result.is_ok(), positive, "native fixture {name}");
        let expected = result
            .ok()
            .map(|bytes| serde_json::from_str::<Value>(&bytes).unwrap());
        cases.push(json!({"name":name,"token":token,"key":key,"context":context,"now_ms":now_ms,"expected":expected}));
    };
    add(
        "google-web",
        signed(header.clone(), claims()),
        public_key.clone(),
        context(),
        true,
    );
    let mut apple = claims();
    apple["iss"] = json!("https://appleid.apple.com");
    let mut apple_context = context();
    apple_context["provider"] = json!("apple");
    add(
        "apple-web",
        signed(header.clone(), apple),
        public_key.clone(),
        apple_context,
        true,
    );
    let mut multiple = claims();
    multiple["aud"] = json!(["web-client", "other"]);
    multiple["azp"] = json!("ios-client");
    let mut ios_context = context();
    ios_context["presenter"] = json!("ios-client");
    add(
        "google-ios-presenter",
        signed(header.clone(), multiple),
        public_key.clone(),
        ios_context,
        true,
    );
    for (field, wrong) in [
        ("nonce", json!("wrong")),
        ("iss", json!("https://evil.example")),
        ("aud", json!("other-client")),
        ("azp", json!("attacker-client")),
        ("exp", json!(1_699_999_999)),
        ("iat", json!(1_700_000_031)),
        ("iat", json!(1_699_999_939)),
        ("sub", json!("")),
        ("exp", json!(1_700_007_201)),
    ] {
        let mut invalid = claims();
        invalid[field] = wrong.clone();
        add(
            &format!("wrong-{field}-{wrong}"),
            signed(header.clone(), invalid),
            public_key.clone(),
            context(),
            false,
        );
    }
    for extra in [
        json!({"alg":"HS256","kid":"test-key"}),
        json!({"alg":"none","kid":"test-key"}),
        json!({"alg":"RS256","kid":"test-key","jku":"https://evil.example/keys"}),
        json!({"alg":"RS256","kid":"test-key","crit":["b64"],"b64":false}),
    ] {
        add(
            &format!("header-{extra}"),
            signed(extra, claims()),
            public_key.clone(),
            context(),
            false,
        );
    }
    let mut tampered = signed(header.clone(), claims()).into_bytes();
    let at = tampered.len() - 2;
    tampered[at] = if tampered[at] == b'A' { b'B' } else { b'A' };
    add(
        "changed-signature",
        String::from_utf8(tampered).unwrap(),
        public_key.clone(),
        context(),
        false,
    );
    let mut wrong_use = public_key.clone();
    wrong_use["use"] = json!("enc");
    add(
        "key-for-encryption",
        signed(header, claims()),
        wrong_use,
        context(),
        false,
    );
    assert_eq!(cases.len(), 18);
    println!(
        "{}",
        json!({"schema":1,"scope":"Synthetic assertion verification only; no provider HTTP, enrollment, recovery or deployment acceptance","public_key":public_key,"cases":cases})
    );
}
