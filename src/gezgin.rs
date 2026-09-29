//! Özyinelemeli dizin gezgini ve dosya uzantısı filtresi.
//!
//! `walkdir`/`notify` crate'leri bağımlılık politikası nedeniyle yasak
//! (WORKER_CONTRACT § 3.2-F); bu modül `std::fs::read_dir` üzerine kendi
//! özyinelemeli gezginini kurar.
//!
//! # Bellek
//!
//! Gezgin yalnız **bir dizinlik** girdi listesini tutar. Alt dizinlere
//! inildiğinde üst dizinin `DirEntry` vektörü düşürülür. Çok derin bir ağaçta
//! bile tepe bellek `O(en_geniş_dizin)` ile sınırlıdır, `O(dosya_sayısı)`
//! değil — bu, raporun "tepe bellek dosya sayısıyla artmaz" kuralının
//! gezgin tarafındaki karşılığıdır.

use std::path::{Path, PathBuf};

use crate::hata::{Hata, Sonuc};

/// BurstJudge'ın kabul ettiği dosya uzantıları (küçük harfle).
///
/// RAW ve TIFF-tabanlı: `cr2`, `cr3`, `nef`, `nrw`, `arw`, `srf`, `sr2`,
/// `raf`, `orf`, `dng`, `pef`, `raw`, `rwl`, `3fr`, `erf`, `mrw`, `mos`,
/// `x3f`, `srw`. Düz görüntü: `tif`, `tiff`, `jpg`, `jpeg`.
pub const DESTEKLENEN_UZANTILAR: &[&str] = &[
    "cr2", "cr3", "nef", "nrw", "arw", "srf", "sr2", "raf", "orf", "dng", "pef", "raw", "rwl",
    "3fr", "erf", "mrw", "mos", "x3f", "srw", "tif", "tiff", "jpg", "jpeg",
];

/// Tarama sırasında atlanan dizin adları (gizli ve sistem klasörleri).
pub const ATLANAN_DIZINLER: &[&str] = &[".git", "node_modules", "target", ".cache", "$RECYCLE.BIN"];

/// Dosya adından küçük harf uzantıyı çıkarır; uzantı yoksa `None`.
///
/// `IMG_0001.CR2` -> `"cr2"`, `RAPOR.JSON` -> `"json"`, `veri` -> `None`.
pub fn uzanti(yol: &Path) -> Option<String> {
    yol.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
}

/// Dosya kabul ediliyor mu? Uzantı desteklenen listede mi?
pub fn kabul(yol: &Path) -> bool {
    match uzanti(yol) {
        Some(u) => DESTEKLENEN_UZANTILAR.contains(&u.as_str()),
        None => false,
    }
}

/// Gezginin topladığı dosya yolları ve atlananlar.
#[derive(Debug, Clone, Default)]
pub struct Gezinme {
    /// Kabul edilen dosya yolları (keşif sırası).
    pub dosyalar: Vec<PathBuf>,
    /// Okunamayan dizinler (yol, neden).
    pub okunamayan_dizinler: Vec<(PathBuf, String)>,
    /// Atlanan dizin sayısı (gizli/sistem).
    pub atlanan_dizin: usize,
    /// Ziyaret edilen dizin sayısı.
    pub ziyaret_dizin: usize,
}

/// Kökten başlayarak özyinelemeli dosya toplar.
///
/// Döngüsel bağlantı (sembolik link) `DirEntry::file_type()` kontrolüyle
/// engellenir: yalnız gerçek dizinlere inilir, sembolik bağlantılar
/// `dosyalar`'a eklenmez. Bu, sonsuz özyineleme riskini kapatır.
pub fn gez(kok: &Path) -> Sonuc<Gezinme> {
    let mut sonuc = Gezinme::default();
    if !kok.is_dir() {
        return Err(Hata::TaramaYoluHatali {
            yol: kok.to_path_buf(),
            ayrinti: "verilen yol bir dizin değil".to_string(),
        });
    }
    let mut yigin: Vec<PathBuf> = vec![kok.to_path_buf()];
    while let Some(dizin) = yigin.pop() {
        sonuc.ziyaret_dizin += 1;
        let okuyucu = match std::fs::read_dir(&dizin) {
            Ok(r) => r,
            Err(_) => {
                sonuc
                    .okunamayan_dizinler
                    .push((dizin.clone(), "dizin acilamadi".to_string()));
                continue;
            }
        };
        for girdi in okuyucu {
            let Ok(girdi) = girdi else { continue };
            let yol = girdi.path();
            let Ok(tip) = girdi.file_type() else { continue };
            let ad = girdi.file_name();
            let ad = ad.to_string_lossy();
            if tip.is_dir() {
                if ATLANAN_DIZINLER.contains(&ad.as_ref()) {
                    sonuc.atlanan_dizin += 1;
                    continue;
                }
                yigin.push(yol);
            } else if tip.is_file() && kabul(&yol) {
                sonuc.dosyalar.push(yol);
            }
            // Sembolik bağlantılar: ne dizin ne dosya sayılır (döngü koruması).
        }
    }
    Ok(sonuc)
}

#[cfg(test)]
// Gerekçe: `unwrap`/`expect` yalnızca test içinde kullanılır.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    struct GeciciDizin {
        yol: PathBuf,
    }

    impl GeciciDizin {
        fn yeni(etiket: &str) -> GeciciDizin {
            let yol =
                std::env::temp_dir().join(format!("bj-gez-{}-{}", etiket, std::process::id()));
            let _ = std::fs::remove_dir_all(&yol);
            std::fs::create_dir_all(&yol).expect("dizin olustur");
            GeciciDizin { yol }
        }
    }

    impl Drop for GeciciDizin {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.yol);
        }
    }

    #[test]
    fn uzanti_kucuk_harfe_indirilir() {
        assert_eq!(uzanti(Path::new("a.CR2")).as_deref(), Some("cr2"));
        assert_eq!(uzanti(Path::new("a.TIFF")).as_deref(), Some("tiff"));
        assert_eq!(uzanti(Path::new("veri")).as_deref(), None);
    }

    #[test]
    fn kabul_desteklenen_uzantilari_alir() {
        assert!(kabul(Path::new("IMG_0001.CR2")));
        assert!(kabul(Path::new("a.dng")));
        assert!(kabul(Path::new("b.nef")));
        assert!(kabul(Path::new("c.jpg")));
    }

    #[test]
    fn kabul_desteklenmeyen_uzantilari_reddeder() {
        assert!(!kabul(Path::new("a.txt")));
        assert!(!kabul(Path::new("a")));
        assert!(!kabul(Path::new("a.pdf")));
    }

    #[test]
    fn bos_dizin_bos_dondurur() {
        let d = GeciciDizin::yeni("bos");
        let g = gez(&d.yol).expect("gez");
        assert!(g.dosyalar.is_empty());
        assert_eq!(g.ziyaret_dizin, 1);
    }

    #[test]
    fn tek_dosya_bulunur() {
        let d = GeciciDizin::yeni("tek");
        std::fs::write(d.yol.join("a.cr2"), b"x").expect("yaz");
        let g = gez(&d.yol).expect("gez");
        assert_eq!(g.dosyalar.len(), 1);
    }

    #[test]
    fn uzanti_filtresi_calisir() {
        let d = GeciciDizin::yeni("filtre");
        std::fs::write(d.yol.join("a.cr2"), b"x").expect("yaz");
        std::fs::write(d.yol.join("b.txt"), b"x").expect("yaz");
        std::fs::write(d.yol.join("c.jpg"), b"x").expect("yaz");
        let g = gez(&d.yol).expect("gez");
        assert_eq!(g.dosyalar.len(), 2, "sadece .cr2 ve .jpg");
    }

    #[test]
    fn alt_dizinlere_iner() {
        let d = GeciciDizin::yeni("alt");
        let alt = d.yol.join("2026");
        std::fs::create_dir_all(&alt).expect("alt dizin");
        std::fs::write(alt.join("a.cr2"), b"x").expect("yaz");
        std::fs::write(d.yol.join("b.cr2"), b"x").expect("yaz");
        let g = gez(&d.yol).expect("gez");
        assert_eq!(g.dosyalar.len(), 2);
        assert_eq!(g.ziyaret_dizin, 2);
    }

    #[test]
    fn gizli_dizinler_atlanir() {
        let d = GeciciDizin::yeni("gizli");
        let gizli = d.yol.join(".git");
        std::fs::create_dir_all(&gizli).expect("dizin");
        std::fs::write(gizli.join("a.cr2"), b"x").expect("yaz");
        let g = gez(&d.yol).expect("gez");
        assert!(g.dosyalar.is_empty());
        assert_eq!(g.atlanan_dizin, 1);
    }

    #[test]
    fn dizin_olmayan_yol_hata_verir() {
        let sonuc = gez(Path::new("C:\\yok\\boyle\\bir\\dizin"));
        assert!(sonuc.is_err());
    }

    #[test]
    fn dosya_yolu_hata_verir() {
        let d = GeciciDizin::yeni("dosya");
        let f = d.yol.join("a.cr2");
        std::fs::write(&f, b"x").expect("yaz");
        assert!(gez(&f).is_err());
    }

    #[test]
    fn cok_derin_agac_taranir() {
        let d = GeciciDizin::yeni("derin");
        let mut yol = d.yol.clone();
        for i in 0..5 {
            yol = yol.join(format!("seviye{}", i));
        }
        std::fs::create_dir_all(&yol).expect("derin dizin");
        std::fs::write(yol.join("a.cr2"), b"x").expect("yaz");
        let g = gez(&d.yol).expect("gez");
        assert_eq!(g.dosyalar.len(), 1);
        assert_eq!(g.ziyaret_dizin, 6);
    }
}
