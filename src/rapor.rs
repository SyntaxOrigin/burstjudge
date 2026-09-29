//! JSON rapor üretimi — gruplar, puanlar, öneri, gerekçe.
//!
//! # Şema (sürüm 1)
//!
//! ```json
//! {
//!   "surum": "0.1.0",
//!   "sema": 1,
//!   "kok": "C:/kart/2026-09-29",
//!   "esik": 8,
//!   "istatistik": { "bulunan_dosya": 12, "okunan_bayt": 3400, ... },
//!   "gruplar": [
//!     { "kimlik": 0, "oneri": "IMG_0042.CR2", "cakismali": false,
//!       "uyeler": [ { "yol": "...", "hash": "a1b2...", "puan": 71.2,
//!                     "bilesenler": [ ... ] } ] }
//!   ],
//!   "atlanan": [ { "yol": "...", "hata": "bigtiff", "ayrinti": "..." } ]
//! }
//! ```
//!
//! Rapor **taşınabilirdir**: yollar mutlak olarak yazılır ama JSON'u başka
//! bir makinede okumak mümkündür; `export` komutu yolları yeniden çözer.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::bhash::AlgisalHash;
use crate::grupla::Gruplama;
use crate::motor::{AtlananDosya, Istatistik, KareKaydi, Motor};
use crate::puan::{Ayarlar, Puan};

/// Rapor şema sürümü. Şema değiştiğinde artar.
pub const SEMA_SURUMU: u32 = 1;

/// Raporun tamamı.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rapor {
    /// Aracın sürümü.
    pub surum: String,
    /// Şema sürümü.
    pub sema: u32,
    /// Taranan kök dizin.
    pub kok: PathBuf,
    /// Kullanılan hamming eşiği.
    pub esik: u32,
    /// Tarama istatistiği.
    pub istatistik: Istatistik,
    /// Gruplar (en çok üyeli olan başta).
    pub gruplar: Vec<GrupRaporu>,
    /// Atlanan dosyalar.
    pub atlanan: Vec<AtlananDosya>,
}

impl Rapor {
    /// Raporu JSON metnine çevirir (okunabilir, 2 boşluk girintili).
    pub fn json_uret(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Toplam grup sayısı.
    pub fn grup_sayisi(&self) -> usize {
        self.gruplar.len()
    }
}

/// Tek bir grubun rapor gösterimi.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrupRaporu {
    /// Grup kimliği (0'dan başlar).
    pub kimlik: usize,
    /// Önerilen kare (puanı en yüksek üye). Grup tek üyeyse de o üyedir.
    pub oneri: PathBuf,
    /// Öneri gerekçesi (tek satır, Türkçe).
    pub gerekce: String,
    /// Grup içinde eşiği aşan çift var mı (zincir birleşme)?
    pub cakismali: bool,
    /// Grubun en büyük iç hamming uzaklığı.
    pub en_uzak_mesafe: u32,
    /// Üyeler (puanı yüksekten düşüğe).
    pub uyeler: Vec<UyeRaporu>,
}

/// Bir grubun tek üyesi.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UyeRaporu {
    /// Dosya yolu.
    pub yol: PathBuf,
    /// Dosya adı.
    pub dosya_adi: String,
    /// Algısal hash (16 ondalık hane); önizleme yoksa `null`.
    pub hash: Option<String>,
    /// Beş bileşenli puan.
    pub puan: Puan,
    /// Toplam puan (tekrar; sıralama için kolaylık).
    pub toplam_puan: f64,
}

/// Tarama + gruplama sonucunu rapora çevirir.
pub fn olustur(
    kok: &Path,
    _motor: &Motor,
    esik: u32,
    kayitlar: &[KareKaydi],
    atlanan: &[AtlananDosya],
    gruplama: &Gruplama,
    _ayarlar: &Ayarlar,
) -> Rapor {
    let istatistik = crate::motor::istatistik(kayitlar, atlanan);
    // Yalnız hash'i olan kareler gruplanır; önizlemesizler her biri tek başına kalır.
    let hashli: Vec<usize> = kayitlar
        .iter()
        .enumerate()
        .filter(|(_, k)| k.hash.is_some())
        .map(|(i, _)| i)
        .collect();
    let oge_liste: Vec<crate::grupla::Oge> = hashli
        .iter()
        .filter_map(|&i| {
            kayitlar[i].hash.map(|h| crate::grupla::Oge {
                sira: i,
                hash: AlgisalHash(h.0),
            })
        })
        .collect();
    let _ = gruplama;

    // Grupları yeniden kur (hash'i olmayanlar tek üyelik grup olarak eklenir).
    let gruplama = crate::grupla::grupla(&oge_liste, esik);
    let mut grup_raporlari: Vec<GrupRaporu> = Vec::new();

    for (grup_sira, uyeler) in gruplama.gruplar.iter().enumerate() {
        let mut uye_raporlari: Vec<UyeRaporu> = uyeler
            .iter()
            .map(|&i| {
                let k = &kayitlar[hashli[i]];
                UyeRaporu {
                    yol: k.yol.clone(),
                    dosya_adi: k.dosya_adi.clone(),
                    hash: k.hash.map(|h| h.onaltilik()),
                    puan: k.puan.clone(),
                    toplam_puan: k.puan.toplam,
                }
            })
            .collect();
        uye_raporlari.sort_by(|a, b| {
            b.toplam_puan
                .partial_cmp(&a.toplam_puan)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let (oneri, gerekce) = oneri_sec(&uye_raporlari);
        let en_uzak = en_uzak_mesafe(&uye_raporlari);
        let cakismali = gruplama
            .cakismalar
            .iter()
            .any(|c| uyeler.contains(&hashli[c.ilk]) && uyeler.contains(&hashli[c.ikinci]));
        grup_raporlari.push(GrupRaporu {
            kimlik: grup_sira,
            oneri,
            gerekce,
            cakismali,
            en_uzak_mesafe: en_uzak,
            uyeler: uye_raporlari,
        });
    }

    // Hash'i olmayan kareler: her biri tek üyelik grup.
    let hashsiz: Vec<&KareKaydi> = kayitlar.iter().filter(|k| k.hash.is_none()).collect();
    for k in hashsiz {
        let uye = UyeRaporu {
            yol: k.yol.clone(),
            dosya_adi: k.dosya_adi.clone(),
            hash: None,
            puan: k.puan.clone(),
            toplam_puan: k.puan.toplam,
        };
        let (oneri, gerekce) = oneri_sec(std::slice::from_ref(&uye));
        grup_raporlari.push(GrupRaporu {
            kimlik: grup_raporlari.len(),
            oneri,
            gerekce,
            cakismali: false,
            en_uzak_mesafe: 0,
            uyeler: vec![uye],
        });
    }

    // Büyük gruplar başta.
    grup_raporlari.sort_by_key(|g| std::cmp::Reverse(g.uyeler.len()));

    Rapor {
        surum: crate::SURUM.to_string(),
        sema: SEMA_SURUMU,
        kok: kok.to_path_buf(),
        esik,
        istatistik,
        gruplar: grup_raporlari,
        atlanan: atlanan.to_vec(),
    }
}

/// Grup içinden en yüksek puanlı kareyi önerir ve gerekçe yazar.
fn oneri_sec(uyeler: &[UyeRaporu]) -> (PathBuf, String) {
    if uyeler.is_empty() {
        return (PathBuf::from("?"), "grup boş".to_string());
    }
    // `uyeler` puana göre sıralıdır; ilk eleman en iyisidir.
    let en_iyi = &uyeler[0];
    let parcalar: Vec<String> = en_iyi
        .puan
        .bilesenler
        .iter()
        .filter_map(|b| b.puan.map(|v| format!("{} {:.0}", b.ad, v)))
        .collect();
    let gerekce = format!(
        "{}: toplam {:.1} puan ({})",
        en_iyi.dosya_adi,
        en_iyi.toplam_puan,
        parcalar.join(", ")
    );
    (en_iyi.yol.clone(), gerekce)
}

/// Grubun en büyük iç hamming uzaklığı (tek üyeli grupta 0).
fn en_uzak_mesafe(uyeler: &[UyeRaporu]) -> u32 {
    let hashler: Vec<AlgisalHash> = uyeler
        .iter()
        .filter_map(|u| {
            u.hash
                .as_ref()
                .and_then(|h| u64::from_str_radix(h, 16).ok())
        })
        .map(AlgisalHash)
        .collect();
    let mut en_uzak = 0u32;
    for i in 0..hashler.len() {
        for j in (i + 1)..hashler.len() {
            let m = hashler[i].hamming(&hashler[j]);
            if m > en_uzak {
                en_uzak = m;
            }
        }
    }
    en_uzak
}

#[cfg(test)]
// Gerekçe: `unwrap`/`expect` yalnızca test içinde kullanılır.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::ornek_veri::Kayit;
    use std::path::PathBuf;

    struct GeciciDizin {
        yol: PathBuf,
    }

    impl GeciciDizin {
        fn yeni(etiket: &str) -> GeciciDizin {
            let yol =
                std::env::temp_dir().join(format!("bj-rapor-{}-{}", etiket, std::process::id()));
            let _ = std::fs::remove_dir_all(&yol);
            std::fs::create_dir_all(&yol).expect("dizin");
            GeciciDizin { yol }
        }
    }

    impl Drop for GeciciDizin {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.yol);
        }
    }

    #[test]
    fn bos_klasor_raporu_bos_gruplarla_cikar() {
        let d = GeciciDizin::yeni("bos");
        let m = Motor::yeni();
        let (kayitlar, atlanan) = m.tara_ayrinti(&d.yol).expect("tara");
        let g = crate::grupla::grupla(&[], 8);
        let r = olustur(
            &d.yol,
            &m,
            8,
            &kayitlar,
            &atlanan,
            &g,
            &Ayarlar::varsayilan(),
        );
        assert_eq!(r.grup_sayisi(), 0);
        assert!(r.json_uret().is_ok());
    }

    #[test]
    fn tek_dosya_tek_grup_olur() {
        let d = GeciciDizin::yeni("tek");
        std::fs::write(
            d.yol.join("a.cr2"),
            crate::ornek_veri::tif_tek(&Kayit::canon()),
        )
        .expect("yaz");
        let m = Motor::yeni();
        let (kayitlar, atlanan) = m.tara_ayrinti(&d.yol).expect("tara");
        let g = crate::grupla::grupla(&[], 8);
        let r = olustur(
            &d.yol,
            &m,
            8,
            &kayitlar,
            &atlanan,
            &g,
            &Ayarlar::varsayilan(),
        );
        assert_eq!(r.grup_sayisi(), 1);
        assert_eq!(r.gruplar[0].uyeler.len(), 1);
        assert!(!r.gruplar[0].cakismali);
    }

    #[test]
    fn ayni_onizlemeli_kareler_tek_grupla_birlesir() {
        let d = GeciciDizin::yeni("ayni");
        for i in 0..3 {
            std::fs::write(
                d.yol.join(format!("a{}.cr2", i)),
                crate::ornek_veri::tif_tek(&Kayit::onizleme_ile(16, 16, 90)),
            )
            .expect("yaz");
        }
        let m = Motor::yeni();
        let (kayitlar, atlanan) = m.tara_ayrinti(&d.yol).expect("tara");
        assert_eq!(kayitlar.len(), 3);
        assert!(kayitlar.iter().all(|k| k.hash.is_some()));
        let g = crate::grupla::grupla(&[], 8);
        let r = olustur(
            &d.yol,
            &m,
            8,
            &kayitlar,
            &atlanan,
            &g,
            &Ayarlar::varsayilan(),
        );
        assert_eq!(r.grup_sayisi(), 1, "aynı görüntü -> tek grup");
        assert_eq!(r.gruplar[0].uyeler.len(), 3);
    }

    #[test]
    fn onizlemesiz_kareler_tek_uyelik_grup_olur() {
        let d = GeciciDizin::yeni("onizlemesiz");
        for i in 0..2 {
            std::fs::write(
                d.yol.join(format!("a{}.cr2", i)),
                crate::ornek_veri::tif_tek(&Kayit::oran()),
            )
            .expect("yaz");
        }
        let m = Motor::yeni();
        let (kayitlar, atlanan) = m.tara_ayrinti(&d.yol).expect("tara");
        let g = crate::grupla::grupla(&[], 8);
        let r = olustur(
            &d.yol,
            &m,
            8,
            &kayitlar,
            &atlanan,
            &g,
            &Ayarlar::varsayilan(),
        );
        assert_eq!(r.grup_sayisi(), 2, "her kare kendi grubunda");
        assert!(r.gruplar.iter().all(|g| g.uyeler[0].hash.is_none()));
    }

    #[test]
    fn oneride_en_yuksek_puanli_kare_secilir() {
        let d = GeciciDizin::yeni("oneri");
        // Düşük ISO ve uzun poz (yüksek puan) vs yüksek ISO.
        let iyi = Kayit {
            iso: Some(100),
            poz_suresi: Some(1.0 / 2000.0),
            odak_mm: Some(50.0),
            onizleme: Some((16, 16, 90)),
            ..Kayit::canon()
        };
        let kotu = Kayit {
            iso: Some(12800),
            poz_suresi: Some(1.0 / 30.0),
            odak_mm: Some(50.0),
            onizleme: Some((16, 16, 90)),
            ..Kayit::canon()
        };
        std::fs::write(d.yol.join("kotu.cr2"), crate::ornek_veri::tif_tek(&kotu)).expect("yaz");
        std::fs::write(d.yol.join("iyi.cr2"), crate::ornek_veri::tif_tek(&iyi)).expect("yaz");
        let m = Motor::yeni();
        let (kayitlar, atlanan) = m.tara_ayrinti(&d.yol).expect("tara");
        let g = crate::grupla::grupla(&[], 8);
        let r = olustur(
            &d.yol,
            &m,
            8,
            &kayitlar,
            &atlanan,
            &g,
            &Ayarlar::varsayilan(),
        );
        assert_eq!(r.grup_sayisi(), 1);
        let oneri_ad = r.gruplar[0]
            .oneri
            .file_name()
            .map(|a| a.to_string_lossy().to_string())
            .unwrap_or_default();
        assert_eq!(oneri_ad, "iyi.cr2", "düşük ISO kazanmalı");
    }

    #[test]
    fn gerekce_metin_icerir() {
        let d = GeciciDizin::yeni("gerekce");
        std::fs::write(
            d.yol.join("a.cr2"),
            crate::ornek_veri::tif_tek(&Kayit::canon()),
        )
        .expect("yaz");
        let m = Motor::yeni();
        let (kayitlar, atlanan) = m.tara_ayrinti(&d.yol).expect("tara");
        let g = crate::grupla::grupla(&[], 8);
        let r = olustur(
            &d.yol,
            &m,
            8,
            &kayitlar,
            &atlanan,
            &g,
            &Ayarlar::varsayilan(),
        );
        let gk = &r.gruplar[0].gerekce;
        assert!(gk.contains("toplam"), "{}", gk);
    }

    #[test]
    fn json_semasinda_alanlar_bulunur() {
        let d = GeciciDizin::yeni("sema");
        std::fs::write(
            d.yol.join("a.cr2"),
            crate::ornek_veri::tif_tek(&Kayit::canon()),
        )
        .expect("yaz");
        let m = Motor::yeni();
        let (kayitlar, atlanan) = m.tara_ayrinti(&d.yol).expect("tara");
        let g = crate::grupla::grupla(&[], 8);
        let r = olustur(
            &d.yol,
            &m,
            8,
            &kayitlar,
            &atlanan,
            &g,
            &Ayarlar::varsayilan(),
        );
        let json = r.json_uret().expect("json");
        for alan in [
            "\"surum\"",
            "\"sema\"",
            "\"kok\"",
            "\"esik\"",
            "\"istatistik\"",
            "\"gruplar\"",
        ] {
            assert!(json.contains(alan), "eksik alan: {}", alan);
        }
    }

    #[test]
    fn rapor_json_donusu_kararli() {
        let d = GeciciDizin::yeni("donus");
        std::fs::write(
            d.yol.join("a.cr2"),
            crate::ornek_veri::tif_tek(&Kayit::canon()),
        )
        .expect("yaz");
        let m = Motor::yeni();
        let (kayitlar, atlanan) = m.tara_ayrinti(&d.yol).expect("tara");
        let g = crate::grupla::grupla(&[], 8);
        let r = olustur(
            &d.yol,
            &m,
            8,
            &kayitlar,
            &atlanan,
            &g,
            &Ayarlar::varsayilan(),
        );
        let json = r.json_uret().expect("json");
        let geri: Rapor = serde_json::from_str(&json).expect("ayrıştır");
        assert_eq!(geri.grup_sayisi(), r.grup_sayisi());
        assert_eq!(geri.esik, 8);
    }

    #[test]
    fn en_uzak_mesafe_tek_uyede_sifirdir() {
        let u = vec![UyeRaporu {
            yol: PathBuf::from("a"),
            dosya_adi: "a".into(),
            hash: Some("0000000000000000".into()),
            puan: Puan {
                bilesenler: Vec::new(),
                toplam: 0.0,
                katilan_bilesen: 0,
            },
            toplam_puan: 0.0,
        }];
        assert_eq!(en_uzak_mesafe(&u), 0);
    }

    #[test]
    fn en_uzak_mesafe_iki_uye_arasini_olcer() {
        let u = vec![
            UyeRaporu {
                yol: PathBuf::from("a"),
                dosya_adi: "a".into(),
                hash: Some("0000000000000000".into()),
                puan: Puan {
                    bilesenler: Vec::new(),
                    toplam: 0.0,
                    katilan_bilesen: 0,
                },
                toplam_puan: 0.0,
            },
            UyeRaporu {
                yol: PathBuf::from("b"),
                dosya_adi: "b".into(),
                hash: Some("000000000000000f".into()),
                puan: Puan {
                    bilesenler: Vec::new(),
                    toplam: 0.0,
                    katilan_bilesen: 0,
                },
                toplam_puan: 0.0,
            },
        ];
        assert_eq!(en_uzak_mesafe(&u), 4);
    }

    #[test]
    fn oneri_bos_grupta_guvenli_donus_yapar() {
        let (yol, gk) = oneri_sec(&[]);
        assert_eq!(yol, PathBuf::from("?"));
        assert!(gk.contains("boş"));
    }
}
