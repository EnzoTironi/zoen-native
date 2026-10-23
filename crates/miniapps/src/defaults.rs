//! Shared built-in game geography and demonstration templates.

use super::*;

// ── MapTap ──

/// Lugares do MapTap: os mesmos para todo mundo no grupo (sorteados pelo dia).
/// (name, country, hint) in pt-BR and en, then lat/lon.
type Place = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    f64,
    f64,
);
const PLACES: &[Place] = &[
    (
        "Liubliana",
        "Ljubljana",
        "Eslovênia",
        "Slovenia",
        "Um pouco de energia de dragão. Ache no globo.",
        "A little dragon energy. Find it on the globe.",
        46.0569,
        14.5058,
    ),
    (
        "Ushuaia",
        "Ushuaia",
        "Argentina",
        "Argentina",
        "A cidade mais ao sul do mundo.",
        "The southernmost city in the world.",
        -54.8019,
        -68.3030,
    ),
    (
        "Fernando de Noronha",
        "Fernando de Noronha",
        "Brasil",
        "Brazil",
        "Ilha no meio do Atlântico, a 350 km da costa.",
        "An island in the middle of the Atlantic, 350 km off the coast.",
        -3.8547,
        -32.4247,
    ),
    (
        "Reykjavík",
        "Reykjavík",
        "Islândia",
        "Iceland",
        "Capital mais ao norte do planeta.",
        "The northernmost capital on the planet.",
        64.1466,
        -21.9426,
    ),
    (
        "Ulan Bator",
        "Ulaanbaatar",
        "Mongólia",
        "Mongolia",
        "Capital mais fria do mundo.",
        "The coldest capital in the world.",
        47.8864,
        106.9057,
    ),
    (
        "Lençóis Maranhenses",
        "Lençóis Maranhenses",
        "Brasil",
        "Brazil",
        "Dunas brancas com lagoas de chuva.",
        "White dunes with rainwater lagoons.",
        -2.4870,
        -43.1280,
    ),
    (
        "Tbilisi",
        "Tbilisi",
        "Geórgia",
        "Georgia",
        "Entre o Mar Negro e o Cáspio.",
        "Between the Black Sea and the Caspian.",
        41.7151,
        44.8271,
    ),
    (
        "Zanzibar",
        "Zanzibar",
        "Tanzânia",
        "Tanzania",
        "Ilha das especiarias no Índico.",
        "The spice island in the Indian Ocean.",
        -6.1659,
        39.2026,
    ),
    (
        "Valparaíso",
        "Valparaíso",
        "Chile",
        "Chile",
        "Morros coloridos de frente para o Pacífico.",
        "Colorful hills facing the Pacific.",
        -33.0472,
        -71.6127,
    ),
    (
        "Hanói",
        "Hanoi",
        "Vietnã",
        "Vietnam",
        "Lagos no meio da cidade e café com ovo.",
        "Lakes in the middle of the city and egg coffee.",
        21.0278,
        105.8342,
    ),
];

pub(super) fn maptap_places(now: i64) -> Value {
    let day = (now / 86_400_000) as usize;
    let en = is_en();
    let picks: Vec<Value> = (0..5)
        .map(|i| {
            let (name_pt, name_en, country_pt, country_en, hint_pt, hint_en, lat, lon) =
                PLACES[(day * 3 + i * 7) % PLACES.len()];
            let (name, country, hint) = if en {
                (name_en, country_en, hint_en)
            } else {
                (name_pt, country_pt, hint_pt)
            };
            json!({ "name": name, "country": country, "hint": hint, "lat": lat, "lon": lon })
        })
        .collect();
    Value::Array(picks)
}

pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0_f64;
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

/// (quem, pontos, rodadas jogadas), do maior para o menor.
pub fn maptap_board(v: &Value) -> Vec<(String, i64, usize)> {
    let mut b: Vec<(String, i64, usize)> = v["guesses"]
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(who, g)| {
                    let rounds = g.as_object().map(|r| r.len()).unwrap_or(0);
                    let pts = g
                        .as_object()
                        .map(|r| r.values().filter_map(|x| x["points"].as_i64()).sum())
                        .unwrap_or(0);
                    (who.clone(), pts, rounds)
                })
                .collect()
        })
        .unwrap_or_default();
    b.sort_by_key(|row| std::cmp::Reverse(row.1));
    b
}

// ── Receita ──

pub(super) fn recipe_default() -> Value {
    if is_en() {
        return json!({
            "title": "Plantain moqueca",
            "subtitle": "Vegetarian · 35 min · from Zoen",
            "baseServings": 2,
            "servings": 3,
            "ingredients": [
                { "id": "i1", "name": "Ripe plantains", "qty": 2.0, "unit": "", "done": false },
                { "id": "i2", "name": "Coconut milk", "qty": 200.0, "unit": "ml", "done": false },
                { "id": "i3", "name": "Red palm oil", "qty": 1.0, "unit": "tbsp", "done": false },
                { "id": "i4", "name": "Tomatoes", "qty": 2.0, "unit": "", "done": false },
                { "id": "i5", "name": "Bell pepper", "qty": 1.0, "unit": "", "done": false },
                { "id": "i6", "name": "Red onion", "qty": 1.0, "unit": "", "done": false },
                { "id": "i7", "name": "Cilantro", "qty": 0.5, "unit": "bunch", "done": false },
                { "id": "i8", "name": "Lime", "qty": 1.0, "unit": "", "done": false }
            ],
            "steps": [
                { "text": "Slice the plantains into thick rounds and season with lime and salt.", "minutes": 0 },
                { "text": "Sauté the onion, pepper and tomato in the palm oil until soft.", "minutes": 8 },
                { "text": "Add the plantain and coconut milk. Cover and simmer on low heat.", "minutes": 12 },
                { "text": "Finish with chopped cilantro and serve with rice and farofa.", "minutes": 0 }
            ],
            "cooking": null,
            "log": []
        });
    }
    json!({
        "title": "Moqueca de banana-da-terra",
        "subtitle": "Vegetariana · 35 min · do Zoen",
        "baseServings": 2,
        "servings": 3,
        "ingredients": [
            { "id": "i1", "name": "Banana-da-terra madura", "qty": 2.0, "unit": "un", "done": false },
            { "id": "i2", "name": "Leite de coco", "qty": 200.0, "unit": "ml", "done": false },
            { "id": "i3", "name": "Azeite de dendê", "qty": 1.0, "unit": "colher", "done": false },
            { "id": "i4", "name": "Tomate", "qty": 2.0, "unit": "un", "done": false },
            { "id": "i5", "name": "Pimentão colorido", "qty": 1.0, "unit": "un", "done": false },
            { "id": "i6", "name": "Cebola roxa", "qty": 1.0, "unit": "un", "done": false },
            { "id": "i7", "name": "Coentro", "qty": 0.5, "unit": "maço", "done": false },
            { "id": "i8", "name": "Limão", "qty": 1.0, "unit": "un", "done": false }
        ],
        "steps": [
            { "text": "Corte a banana em rodelas grossas e tempere com limão e sal.", "minutes": 0 },
            { "text": "Refogue cebola, pimentão e tomate no dendê até murchar.", "minutes": 8 },
            { "text": "Junte a banana e o leite de coco. Tampe e cozinhe em fogo baixo.", "minutes": 12 },
            { "text": "Finalize com coentro picado e sirva com arroz e farofa.", "minutes": 0 }
        ],
        "cooking": null,
        "log": []
    })
}

/// The three Bay Area trails of the hike demo: id, name, drive minutes from SF, hike hours.
/// Routes, photos and copy live in the mini-app bundle (miniapps/hike).
pub const HIKE_TRAILS: [(&str, &str, i64, i64); 3] = [
    ("tomales", "Tomales Point", 90, 5),
    ("steep", "Steep Ravine", 45, 3),
    ("lands", "Lands End", 25, 2),
];

pub(super) fn hike_name(id: &str) -> Option<&'static str> {
    // English ids stay stable; labels follow the core language.
    match id {
        "tomales" => Some(if is_en() {
            "Tomales Point"
        } else {
            "Praia do Sono"
        }),
        "steep" => Some(if is_en() {
            "Steep Ravine"
        } else {
            "Pedra Grande"
        }),
        "lands" => Some(if is_en() {
            "Lands End"
        } else {
            "Pico do Jaraguá"
        }),
        _ => None,
    }
}

pub(super) fn st_day(st: &Value) -> String {
    st["day"].as_str().unwrap_or("").to_lowercase()
}

/// 8:15, 10:00, 3:00 (12-hour in English, 24-hour in Portuguese).
pub(super) fn clock(minutes: i64) -> String {
    let (h, m) = (minutes / 60 % 24, minutes % 60);
    if is_en() {
        format!("{}:{m:02}", if h % 12 == 0 { 12 } else { h % 12 })
    } else {
        format!("{h}:{m:02}")
    }
}
