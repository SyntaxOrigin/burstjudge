//! Örnek burst klasörü üretir — README'deki komutların gerçek çıktısını üretmek için.
//!
//! Kullanım:
//!
//! ```text
//! cargo run --example ornek_klasor -- <hedef-klasor> [kare-sayisi]
//! ```
//!
//! Üretilen dosyalar gerçek TIFF/EXIF yapısındadır ve gömülü baseline JPEG
//! önizleme taşır; `burstjudge` ile tam olarak aynı şekilde okunurlar.
//!
//! Kütüphane dışında koda çalıştıran kod **değildir** (kural üretici `build.rs`
//! yasağı yalnız derleme zamanı içindir); bu bir `examples/` betiğidir.

use std::path::Path;

use burstjudge::jpeg_test_veri::Desen;
use burstjudge::ornek_veri::Kayit;

fn main() {
    let mut args = std::env::args().skip(1);
    let hedef = args.next().unwrap_or_else(|| "ornek-burst".to_string());
    let adet: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(6);
    let kok = Path::new(&hedef);
    if let Err(e) = std::fs::create_dir_all(kok) {
        eprintln!("hata: {} oluşturulamadı: {}", hedef, e);
        std::process::exit(1);
    }

    // Üç kompozisyon grubu. Her grubun tüm kareleri **aynı deseni** taşır, bu
    // yüzden algısal hash'leri aynıdır ve gruplama gerçekten "grup" üretir.
    // Desenler 8×8 kutu ortalamasından sağ çıkan (yani hash'e giren) yapılar
    // olacak şekilde seçilmiştir: 8 piksel periyotlu şeritler ortalama
    // alındığında düzleşip hash'i sıfırlardı.
    let kompozisyonlar: [(Desen, &str); 3] = [
        (
            Desen::DikeySerit {
                periyot: 16,
                koyu: 30,
                acik: 200,
            },
            "manzara",
        ),
        (
            Desen::Damali {
                hucre: 8,
                koyu: 40,
                acik: 210,
            },
            "portre",
        ),
        (
            Desen::Radyal {
                merkez_gri: 240,
                kenar_gri: 25,
            },
            "mimari",
        ),
    ];
    let grup_boyu = (adet / 3).max(1);
    let mut yazilan = 0usize;
    for (grup_no, (desen, etiket)) in kompozisyonlar.iter().enumerate() {
        for kare in 0..grup_boyu {
            let kayit = Kayit {
                make: "Canon".to_string(),
                model: "Test Kamera".to_string(),
                // Burst içinde saniye saniye ilerleyen çekim dizisi.
                // Biçim Exif 2.3'ün "YYYY:MM:DD HH:MM:SS" kuralına uyar.
                zaman: Some(format!("2026:09:29 10:30:{:02}", grup_no * 9 + kare * 3)),
                // Kare sırasıyla poz uzar (daha uzun poz = hareket riski artar).
                poz_suresi: Some(1.0 / (200.0 * (1 + kare) as f32)),
                diyafram: Some(2.8),
                iso: Some(100 * (1 + kare) as u32),
                odak_mm: Some(35.0 + kare as f32 * 5.0),
                yon_kodu: Some(1),
                genislik: Some(6000),
                yukseklik: Some(4000),
                onizleme: None,
                onizleme_deseni: Some(*desen),
                onizlemeyi_boz: false,
            };
            let ad = format!("IMG_{}_{:04}.CR2", etiket, kare);
            let yol = kok.join(&ad);
            // Gerçek RAW dosyalarının boyutunu taklit etmek için dosya sonuna
            // "ham sensör verisi" doldurulur. Tarama bu bölümü **okumaz**;
            // kısmi okuma iddiasının demo üzerinde görünmesini sağlar.
            let mut veri = burstjudge::ornek_veri::tif_tek(&kayit);
            veri.resize(2 * 1024 * 1024, 0x5A);
            if let Err(e) = std::fs::write(&yol, veri) {
                eprintln!("hata: {} yazılamadı: {}", yol.display(), e);
                std::process::exit(1);
            }
            yazilan += 1;
        }
    }

    // Bir de tamamen bozuk dosya: taramanın devam ettiğini göstermek için.
    let _ = std::fs::write(kok.join("BOGUK_0000.CR2"), b"bu bir TIFF dosyasi degil");

    println!("{} adet örnek kare yazıldı: {}", yazilan, kok.display());
}
