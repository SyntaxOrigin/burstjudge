//! BurstJudge — burst ve çoklu çekim karelerini gruplayan fotoğraf triyaj aracı.
//!
//! # Ürün vaadi
//!
//! Arka arkaya çekilmiş yüzlerce kareyi tarar, gömülü önizlemelerden algısal
//! hash üretir, benzer kompozisyonları gruplar ve her gruptan en güçlü kareyi
//! **önerir**. Hiçbir dosyayı silmez, taşımaz veya tam çözmez.
//!
//! # Katmanlar
//!
//! | Modül | Sorumluluk |
//! |---|---|
//! | [`hata`] | Hata tipi, `Display`/`Error` uygulamaları |
//! | [`tiff`] | TIFF/EXIF IFD ayrıştırma, kademeli ve döngü korumalı |
//! | [`meta`] | EXIF alanlarının anlamsal çözümü (tarih, sayılar, yön) |
//! | [`onizleme`] | Gömülü JPEG önizleme çıkarımı (çözmeden) |
//! | [`jpeg`] | Baseline JPEG luma kod çözücü (kendi yazıldı) |
//! | [`bhash`] | DCT tabanlı algısal hash (kendi DCT'mizle) |
//! | [`grupla`] | Union-find gruplama, hamming eşiği, çakışma tespiti |
//! | [`puan`] | Metadata tabanlı beş bileşenli puanlama |
//! | [`gezgin`] | Özyinelemeli dizin gezgini ve uzantı filtresi |
//! | [`motor`] | Tarama motoru: dosya → metadatası → hash → puan |
//! | [`rapor`] | JSON rapor şeması |
//! | [`aktar`] | Kaynağa dokunmadan dışa aktarma |
//!
//! # Bellek sözleşmesi
//!
//! Hiçbir dosya belleğe tümüyle alınmaz. Tarama kademelidir: dizin okunur,
//! yalnız ihtiyaç duyulan ~20 EXIF değeri okunur, gömülü önizleme çözülür ve
//! **serbest bırakılır**. Görüntü pikselleri saklanmaz; rapor yalnız hash,
//! puan, boyut ve yol tutar. `motor::Istatistik::okuma_orani` bu sözleşmenin
//! ölçülebilir kanıtıdır.
//!
//! # Güvenlik sınırları
//!
//! - BigTIFF reddedilir.
//! - IFD girdi sayısı, zincir adımı ve etiket değer uzunluğu sınırlıdır.
//! - IFD offset'leri dosya sınırına göre doğrulanır; ziyaret kümesi döngüyü keser.
//! - JPEG boyut ve piksel sayısı üst sınırı vardır.
//!
//! # Sapma notu
//!
//! Rapor (LibRaw + Exiv2 + OpenCV) yerine saf Rust uygulama seçilmiştir
//! (MANIFEST kartı 05, karar D-005/D-008). Tam çözüm, demosaic, yüz tespiti
//! ve dHash bu MVP'de yoktur; nedenleri README'nin "Bilinen Sınırlamalar"
//! bölümündedir.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::unwrap_used, clippy::expect_used)]

pub mod aktar;
pub mod bhash;
pub mod gezgin;
pub mod grupla;
pub mod hata;
pub mod jpeg;
pub mod jpeg_test_veri;
pub mod meta;
pub mod motor;
pub mod onizleme;
pub mod ornek_veri;
pub mod puan;
pub mod rapor;
pub mod tiff;

pub use aktar::kopyala;
pub use bhash::AlgisalHash;
pub use grupla::{grupla, Gruplama, Oge};
pub use hata::{Hata, Sonuc};
pub use motor::{KareKaydi, Motor};
pub use puan::{Ayarlar, Puan};
pub use rapor::Rapor;

/// Aracın sürüm dizesi.
pub const SURUM: &str = env!("CARGO_PKG_VERSION");
