//! Entegrasyon testleri için ortak yardımcılar.
//!
//! `tempfile` crate'i bağımlılık politikası nedeniyle yasaktır
//! (WORKER_CONTRACT § 3.2-F); bu modül kendi geçici dizin yöneticisini yazar.
//! Benzersizlik, etiket + süreç kimliği + atomik sayaç ile sağlanır; rastgelelik
//! crate'i kullanılmaz.
//!
//! Her iki entegrasyon test dosyası da bu modülü `mod yardimci;` ile bildirir;
//! yalnız birinde kullanılan yardımcılarda `#[allow(dead_code)]` vardır çünkü
//! her dosya modülü ayrı derlenir.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use burstjudge::ornek_veri::Kayit;
use burstjudge::Motor;

/// Test içinde geçici dosya/dizin üreten, `Drop` ile temizleyen kapsayıcı.
pub struct GeciciDizin {
    yol: PathBuf,
}

impl GeciciDizin {
    /// `std::env::temp_dir()` altında benzersiz bir dizin oluşturur.
    ///
    /// Aynı etiketle ikinci kez çağrılırsa eski içerik önce silinir.
    pub fn yeni(etiket: &str) -> std::io::Result<Self> {
        static SAYAC: AtomicU64 = AtomicU64::new(0);
        let sira = SAYAC.fetch_add(1, Ordering::Relaxed);
        let yol = std::env::temp_dir().join(format!(
            "burstjudge-test-{}-{}-{}",
            etiket,
            std::process::id(),
            sira
        ));
        let _ = std::fs::remove_dir_all(&yol);
        std::fs::create_dir_all(&yol)?;
        Ok(Self { yol })
    }

    /// Dizin içine göreli yol döndürür.
    pub fn yol(&self) -> &Path {
        &self.yol
    }

    /// Dizin içine bir dosya adı döndürür.
    pub fn dosya(&self, ad: &str) -> PathBuf {
        self.yol.join(ad)
    }
}

impl Drop for GeciciDizin {
    fn drop(&mut self) {
        // Temizlik başarısız olsa da testi düşürmemeli; `let _ =` bilinçlidir
        // ve WORKER_CONTRACT § 5.3 tarafından bu bağlamda öngörülmüştür.
        let _ = std::fs::remove_dir_all(&self.yol);
    }
}

/// Dizine bir TIFF/EXIF örnek dosyası yazar ve yolunu döndürür.
pub fn ornek_yaz(dizin: &GeciciDizin, ad: &str, kayit: &Kayit) -> PathBuf {
    let yol = dizin.dosya(ad);
    std::fs::write(&yol, burstjudge::ornek_veri::tif_tek(kayit)).expect("ornek yaz");
    yol
}

/// Verilen baytları dizine dosya olarak yazar.
pub fn ham_yaz(dizin: &GeciciDizin, ad: &str, baytlar: &[u8]) -> PathBuf {
    let yol = dizin.dosya(ad);
    std::fs::write(&yol, baytlar).expect("ham yaz");
    yol
}

/// Bir klasöre `adet` adet aynı görüntülü önizlemeli kare yazar (burst taklidi).
///
/// Tüm kareler aynı önizlemeyi taşır; hash'leri aynı olmalıdır.
pub fn burst_yaz(
    dizin: &GeciciDizin,
    onek: &str,
    adet: usize,
    onizleme: (u16, u16, u8),
) -> Vec<PathBuf> {
    (0..adet)
        .map(|i| {
            let ad = format!("{}{}.cr2", onek, i);
            ornek_yaz(
                dizin,
                &ad,
                &Kayit::onizleme_ile(onizleme.0, onizleme.1, onizleme.2),
            )
        })
        .collect()
}

/// Boş geçici dizinde motorla tarar.
pub fn tara(
    kok: &Path,
) -> burstjudge::Sonuc<(
    Vec<burstjudge::KareKaydi>,
    Vec<burstjudge::motor::AtlananDosya>,
)> {
    Motor::yeni().tara_ayrinti(kok)
}
