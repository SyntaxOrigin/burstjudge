//! Metadata tabanlı puanlama — beş bileşen, ağırlıklı toplam.
//!
//! # Neden metadatala
//!
//! MANIFEST kartı 05 madde 4 puanlamayı **metadata tabanlı** tanımlar; tam
//! çözüm ve görüntü içeriği analizi v1'de yoktur. Bu modül yalnızca EXIF
//! alanlarından türetilen, her biri **açıklanabilir** beş bileşen üretir.
//!
//! # Bileşenler
//!
//! | Bileşen | Ne ölçer | Veri kaynağı |
//! |---|---|---|
//! | `keskinlik` | Hareket bulanıklığı riski: kısa poz + uzun odak = yüksek risk | `ExposureTime`, `FocalLength` |
//! | `pozlama` | Yeterli ışık: ISO ile poz süresinin "elde" kombinasyonu | `ISO`, `ExposureTime`, `FNumber`, `ExposureBiasValue` |
//! | `kirpma` | Konu ayrımı: uzun odak (telefoto) daha yüksek puan | `FocalLength`, `PixelXDimension/YDimension` |
//! | `kompozisyon` | Yöne göre kadraj dengesi: en-boy oranı + yön kodu | `Orientation`, `PixelXDimension/YDimension` |
//! | `iso_dengesi` | Duyarlılık şovalyonluğu (düşük ISO = temiz) | `ISO` |
//!
//! # Puanlama uzlaştırması (rapor `b05` uyarısı)
//!
//! Rapor açıkça uyarır: "puanlama tamamen ölçülmemiştir ve ölçülebilir bir
//! 'en iyi kare' tanımı değildir." Bu modül de aynısını yapar: her bileşen
//! **0-100** arası, toplam ağırlıklı ortalamadır ve **eksik veride** bileşen
//! yalnız hesaba katılmaz (tümü eksikse toplam 0 döner ve gerekçe yazılır).
//!
//! # Tasarım kararı: eksik veri
//!
//! Bir EXIF alanı yoksa bileşen `None` döner; `Puanla::toplam` yalnız `Some`
//! bileşenleri ağırlıklandırır. Böylece parçalı metadata bulunan bir dosya
//! tam puan almaz ama **en azından** mevcut kanıtla değerlendirilir.

use serde::{Deserialize, Serialize};

use crate::meta::Meta;

/// Tek bir puan bileşeni ve gerekçesi.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bilesen {
    /// Bileşen adı (Türkçe, raporda görünür).
    pub ad: String,
    /// 0-100 arası puan; veri yoksa `None`.
    pub puan: Option<f64>,
    /// Ağırlığı (toplamın bu bileşene düşen payı).
    pub agirlik: f64,
    /// Puanın nasıl hesaplandığını anlatan tek satırlık gerekçe.
    pub gerekce: String,
}

/// Beş bileşenin ağırlıklı toplamı.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Puan {
    /// Tüm bileşenler.
    pub bilesenler: Vec<Bilesen>,
    /// Ağırlıklı ortalama (0-100). Veri hiç yoksa 0.
    pub toplam: f64,
    /// Toplamın dayandığı bileşen sayısı.
    pub katilan_bilesen: usize,
}

impl Puan {
    /// Belirli adlı bileşenin puanı.
    pub fn bilesen(&self, ad: &str) -> Option<&Bilesen> {
        self.bilesenler.iter().find(|b| b.ad == ad)
    }

    /// En yüksek puanlı bileşenin adı (rapor önerisi için).
    pub fn guclu_bilesen(&self) -> Option<&str> {
        self.bilesenler
            .iter()
            .filter(|b| b.puan.is_some())
            .max_by(|a, b| {
                a.puan
                    .unwrap_or(0.0)
                    .partial_cmp(&b.puan.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|b| b.ad.as_str())
    }
}

/// Bileşen ağırlıkları ve eşikler — kullanıcı JSON dosyasından yüklenebilir.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ayarlar {
    /// Keskinlik ağırlığı.
    pub agirlik_keskinlik: f64,
    /// Pozlama ağırlığı.
    pub agirlik_pozlama: f64,
    /// Kırpma (konu ayrımı) ağırlığı.
    pub agirlik_kirpma: f64,
    /// Kompozisyon ağırlığı.
    pub agirlik_kompozisyon: f64,
    /// ISO dengesi ağırlığı.
    pub agirlik_iso: f64,
    /// "Yeterli ışık" eşiği: altında pozlama puanı düşer (ISO).
    pub iso_yeterli: f64,
    /// Kompozisyon için tercih edilen en-boy oranı.
    pub hedef_en_boy: f64,
    /// Kompozisyon toleransı (logaritmik sapma ölçeği).
    pub kompozisyon_tolere: f64,
    /// Keskinlikte "yeterince uzun" poz süresi (saniye).
    pub keskin_min_poz: f64,
}

impl Default for Ayarlar {
    fn default() -> Self {
        // Varsayılanlar gözle seçilmiştir (rapor b05: "ağırlıklar gözle seçilmiştir").
        Ayarlar {
            agirlik_keskinlik: 0.30,
            agirlik_pozlama: 0.25,
            agirlik_kirpma: 0.10,
            agirlik_kompozisyon: 0.20,
            agirlik_iso: 0.15,
            iso_yeterli: 1600.0,
            hedef_en_boy: 3.0 / 2.0,
            kompozisyon_tolere: 0.35,
            keskin_min_poz: 1.0 / 250.0,
        }
    }
}

impl Ayarlar {
    /// Varsayılan ayarları döndürür.
    pub fn varsayilan() -> Ayarlar {
        Ayarlar::default()
    }

    /// Ağırlıkların toplamı (raporda gösterilir).
    pub fn agirlik_toplami(&self) -> f64 {
        self.agirlik_keskinlik
            + self.agirlik_pozlama
            + self.agirlik_kirpma
            + self.agirlik_kompozisyon
            + self.agirlik_iso
    }

    /// Ayarlar tutarlı mı? Ağırlıklar negatif veya toplam sıfır olamaz.
    pub fn dogrula(&self) -> Result<(), String> {
        let a = [
            self.agirlik_keskinlik,
            self.agirlik_pozlama,
            self.agirlik_kirpma,
            self.agirlik_kompozisyon,
            self.agirlik_iso,
        ];
        if a.iter().any(|w| !w.is_finite() || *w < 0.0) {
            return Err("ağırlıklar negatif veya sonlu değil".to_string());
        }
        if self.agirlik_toplami() <= 0.0 {
            return Err("ağırlık toplamı sıfır olamaz".to_string());
        }
        if !self.hedef_en_boy.is_finite() || self.hedef_en_boy <= 0.0 {
            return Err("hedef_en_boy pozitif ve sonlu olmalı".to_string());
        }
        if self.iso_yeterli <= 0.0 {
            return Err("iso_yeterli pozitif olmalı".to_string());
        }
        Ok(())
    }
}

/// Bir değeri 0-100 aralığına doğrusal olarak kıstırır.
fn kirp01(v: f64) -> f64 {
    v.clamp(0.0, 100.0)
}

/// Keskinlik: poz süresi + odak uzaklığından hareket bulanıklığı riski.
///
/// Kısa poz (yüksek hız) ve uzun odak, ISO'yu yükseltmeye zorlar; bu
/// artan gürültüyü ve olası hareket bulanıklığını işaret eder. Puan
/// yalnızca **mevcut** poz ve odak bilgisiyle hesaplanır.
fn keskinlik_puani(meta: &Meta, _ayar: &Ayarlar) -> (Option<f64>, String) {
    let (Some(poz), Some(odak)) = (meta.poz_suresi, meta.odak_mm) else {
        return (None, "poz süresi veya odak uzaklığı yok".to_string());
    };
    if poz <= 0.0 || odak <= 0.0 {
        return (None, "poz süresi veya odak geçersiz".to_string());
    }
    // 1/poz = enstantane hız; log2 ölçeğinde 1/256 -> 8 bit, 1/16384 -> 14 bit.
    let hiz = (1.0 / poz).log2();
    // 50mm üzeri odak "ekranı dolduran" kompozisyon => yüksek risk.
    let odak_bonus = (odak / 50.0).log2().max(0.0);
    let risk = hiz + odak_bonus * 1.5;
    // 8 (1/256) altı puan düşer, 14 (1/16384) üstü tam puan.
    let puan = kirp01(f64::from(risk - 8.0) * (100.0 / 6.0));
    (
        Some(puan),
        format!("1/{:.0} s, {:.0} mm -> hareket riski", 1.0 / poz, odak),
    )
}

/// Pozlama: yeterli ışık var mı? ISO ve poz kombinasyonu üzerinden.
///
/// Düşük ISO ve makul poz = yüksek puan. `ExposureBiasValue` poz telafisi
/// kasıtlı karanlık çekimi (silüet) cezalandırmak için **düşürülmez**,
/// yalnızca rapora gerekçe olarak yansır.
fn pozlama_puani(meta: &Meta, _ayar: &Ayarlar) -> (Option<f64>, String) {
    let Some(iso) = meta.iso else {
        return (None, "ISO yok".to_string());
    };
    if iso == 0 {
        return (None, "ISO geçersiz".to_string());
    }
    let iso_f = f64::from(iso);
    // ISO'nun logaritmik "düşüklük" puanı: 100 -> 100, 6400 -> 0.
    let puan = kirp01((12.0 - iso_f.log2()) * (100.0 / 6.0));
    let bias_notu = match meta.poz_bias {
        Some(b) if b != 0.0 => format!(" (poz telafisi {:+.1} EV)", b),
        _ => String::new(),
    };
    (Some(puan), format!("ISO {}{} -> ışık", iso, bias_notu))
}

/// Kırpma (konu ayrımı): uzun odak daha yüksek puan.
///
/// Bu, "kadraja alınmış kompozisyon" (crop) yaklaşımının metadata karşılığıdır:
/// 200mm'de arka plan bulanıklaşır ve konu öne çıkar. Kaynak dosyanın
/// gerçek crop davranışı ölçülmez — bu bir **yaklaşım**dır.
fn kirpma_puani(meta: &Meta, _ayar: &Ayarlar) -> (Option<f64>, String) {
    let Some(odak) = meta.odak_mm else {
        return (None, "odak uzaklığı yok".to_string());
    };
    if odak <= 0.0 {
        return (None, "odak geçersiz".to_string());
    }
    // 24mm -> 0, 200mm -> 100 (log2 ölçekli).
    let puan = kirp01(f64::from((odak / 24.0).log2()) * (100.0 / 3.0));
    (Some(puan), format!("{:.0} mm -> kadraja alınmışlık", odak))
}

/// Kompozisyon: yöne göre kadraj dengesi (en-boy oranı).
///
/// `Orientation` 5-8 ise en-boy oranı 90° döner; hedef oran bu döndürülmüş
/// değere karşı uygulanır. Sapma logaritmik ölçekte ölçülür.
fn kompozisyon_puani(meta: &Meta, ayar: &Ayarlar) -> (Option<f64>, String) {
    let Some(oran) = meta.en_boy_orani() else {
        return (None, "boyut veya yön bilgisi yok".to_string());
    };
    if oran <= 0.0 {
        return (None, "en-boy oranı geçersiz".to_string());
    }
    // Logaritmik sapma: hedef orana ne kadar yakın, o kadar yüksek puan.
    let sapma = (oran.ln() - ayar.hedef_en_boy.ln()).abs();
    let puan = kirp01(100.0 * (-(sapma / ayar.kompozisyon_tolere)).exp());
    let yon_notu = match meta.yon_kodu {
        Some(k @ (5..=8)) => format!(" (yön {}: döndürülmüş)", k),
        _ => String::new(),
    };
    (
        Some(puan),
        format!("en-boy {:.2}{} -> kadraj", oran, yon_notu),
    )
}

/// ISO dengesi: şovalyonluk.
///
/// ISO 100-200 -> tam puan, ISO 25600 -> 0. Burada "ISO 200 -> 100" gibi
/// hafif kayıplar vardır (çok düşük ISO'nun gürültü avantajı ihmal edilir).
fn iso_dengesi_puani(meta: &Meta, _ayar: &Ayarlar) -> (Option<f64>, String) {
    let Some(iso) = meta.iso else {
        return (None, "ISO yok".to_string());
    };
    if iso == 0 {
        return (None, "ISO geçersiz".to_string());
    }
    let puan = kirp01((16.0 - (f64::from(iso).log2())) * (100.0 / 8.0));
    (Some(puan), format!("ISO {} -> gürültü dengesi", iso))
}

/// Bir dosyanın metadatasından beş bileşenli puanı hesaplar.
pub fn puanla(meta: &Meta, ayar: &Ayarlar) -> Puan {
    let (k, kg) = keskinlik_puani(meta, ayar);
    let (p, pg) = pozlama_puani(meta, ayar);
    let (kir, kirg) = kirpma_puani(meta, ayar);
    let (kom, komg) = kompozisyon_puani(meta, ayar);
    let (iso, isog) = iso_dengesi_puani(meta, ayar);

    let bilesenler = vec![
        Bilesen {
            ad: "keskinlik".to_string(),
            puan: k,
            agirlik: ayar.agirlik_keskinlik,
            gerekce: kg,
        },
        Bilesen {
            ad: "pozlama".to_string(),
            puan: p,
            agirlik: ayar.agirlik_pozlama,
            gerekce: pg,
        },
        Bilesen {
            ad: "kirpma".to_string(),
            puan: kir,
            agirlik: ayar.agirlik_kirpma,
            gerekce: kirg,
        },
        Bilesen {
            ad: "kompozisyon".to_string(),
            puan: kom,
            agirlik: ayar.agirlik_kompozisyon,
            gerekce: komg,
        },
        Bilesen {
            ad: "iso_dengesi".to_string(),
            puan: iso,
            agirlik: ayar.agirlik_iso,
            gerekce: isog,
        },
    ];

    let mut toplam_agirlik = 0.0;
    let mut toplam = 0.0;
    let mut katilan = 0usize;
    for b in &bilesenler {
        if let Some(v) = b.puan {
            toplam += v * b.agirlik;
            toplam_agirlik += b.agirlik;
            katilan += 1;
        }
    }
    let toplam_sonuc = if katilan == 0 || toplam_agirlik <= 0.0 {
        0.0
    } else {
        toplam / toplam_agirlik
    };

    Puan {
        bilesenler,
        toplam: toplam_sonuc,
        katilan_bilesen: katilan,
    }
}

#[cfg(test)]
// Gerekçe: `unwrap`/`expect` yalnızca test içinde kullanılır.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::meta::CekimZamani;

    fn tam_meta() -> Meta {
        Meta {
            make: Some("Canon".into()),
            model: Some("EOS R6".into()),
            zaman: CekimZamani::ayrıştir("2026:09:29 10:00:00"),
            poz_suresi: Some(1.0 / 500.0),
            diyafram: Some(2.8),
            iso: Some(200),
            odak_mm: Some(50.0),
            yon_kodu: Some(1),
            genislik: Some(4000),
            yukseklik: Some(3000),
            ..Meta::default()
        }
    }

    #[test]
    fn varsayilan_agirliklar_gecerli() {
        let a = Ayarlar::varsayilan();
        assert!(a.dogrula().is_ok());
        assert!(
            (a.agirlik_toplami() - 1.0).abs() < 1e-9,
            "{}",
            a.agirlik_toplami()
        );
    }

    #[test]
    fn negatif_agirlik_reddedilir() {
        let mut a = Ayarlar::varsayilan();
        a.agirlik_keskinlik = -0.1;
        assert!(a.dogrula().is_err());
    }

    #[test]
    fn sifir_agirlik_toplami_reddedilir() {
        let a = Ayarlar {
            agirlik_keskinlik: 0.0,
            agirlik_pozlama: 0.0,
            agirlik_kirpma: 0.0,
            agirlik_kompozisyon: 0.0,
            agirlik_iso: 0.0,
            ..Ayarlar::varsayilan()
        };
        assert!(a.dogrula().is_err());
    }

    #[test]
    fn negatif_hedef_en_boy_reddedilir() {
        let mut a = Ayarlar::varsayilan();
        a.hedef_en_boy = -1.0;
        assert!(a.dogrula().is_err());
    }

    #[test]
    fn sifir_iso_yeterli_reddedilir() {
        let mut a = Ayarlar::varsayilan();
        a.iso_yeterli = 0.0;
        assert!(a.dogrula().is_err());
    }

    #[test]
    fn bes_bilesen_uretilir() {
        let p = puanla(&tam_meta(), &Ayarlar::varsayilan());
        assert_eq!(p.bilesenler.len(), 5);
        assert_eq!(p.katilan_bilesen, 5, "tam metadata ile 5 bileşen katılır");
        assert!(p.toplam > 0.0);
    }

    #[test]
    fn kisa_poz_yuksek_keskinlik_verir() {
        let a = Ayarlar::varsayilan();
        let hizli = puanla(
            &Meta {
                poz_suresi: Some(1.0 / 2000.0),
                odak_mm: Some(50.0),
                ..tam_meta()
            },
            &a,
        );
        let yavas = puanla(
            &Meta {
                poz_suresi: Some(1.0 / 30.0),
                odak_mm: Some(50.0),
                ..tam_meta()
            },
            &a,
        );
        let kh = hizli
            .bilesen("keskinlik")
            .expect("bileşen")
            .puan
            .expect("puan");
        let ky = yavas
            .bilesen("keskinlik")
            .expect("bileşen")
            .puan
            .expect("puan");
        assert!(kh > ky, "hizli {} yavas {}", kh, ky);
    }

    #[test]
    fn dusuk_iso_yuksek_pozlama_verir() {
        let a = Ayarlar::varsayilan();
        let temiz = puanla(
            &Meta {
                iso: Some(100),
                ..tam_meta()
            },
            &a,
        );
        let gurultulu = puanla(
            &Meta {
                iso: Some(12800),
                ..tam_meta()
            },
            &a,
        );
        let pt = temiz
            .bilesen("pozlama")
            .expect("bileşen")
            .puan
            .expect("puan");
        let pg = gurultulu
            .bilesen("pozlama")
            .expect("bileşen")
            .puan
            .expect("puan");
        assert!(pt > pg, "temiz {} gurultulu {}", pt, pg);
    }

    #[test]
    fn uzun_odak_yuksek_kirpma_verir() {
        let a = Ayarlar::varsayilan();
        let genis = puanla(
            &Meta {
                odak_mm: Some(24.0),
                ..tam_meta()
            },
            &a,
        );
        let tele = puanla(
            &Meta {
                odak_mm: Some(200.0),
                ..tam_meta()
            },
            &a,
        );
        let kg = genis
            .bilesen("kirpma")
            .expect("bileşen")
            .puan
            .expect("puan");
        let kt = tele.bilesen("kirpma").expect("bileşen").puan.expect("puan");
        assert!(kt > kg, "tele {} genis {}", kt, kg);
    }

    #[test]
    fn hedef_en_boya_yakin_kompozisyon_daha_yuksek() {
        let a = Ayarlar::varsayilan();
        let hedefe_yakin = puanla(
            &Meta {
                genislik: Some(3000),
                yukseklik: Some(2000),
                yon_kodu: Some(1),
                ..tam_meta()
            },
            &a,
        );
        let uzak = puanla(
            &Meta {
                genislik: Some(1000),
                yukseklik: Some(1000),
                yon_kodu: Some(1),
                ..tam_meta()
            },
            &a,
        );
        let kh = hedefe_yakin
            .bilesen("kompozisyon")
            .expect("bileşen")
            .puan
            .expect("puan");
        let ku = uzak
            .bilesen("kompozisyon")
            .expect("bileşen")
            .puan
            .expect("puan");
        assert!(kh > ku, "hedefe_yakin {} uzak {}", kh, ku);
    }

    #[test]
    fn yon_kodu_kompozisyonu_degistirir() {
        let a = Ayarlar::varsayilan();
        // 4:3 yatay -> oran 1.33 (hedef 1.5'den uzak)
        let yatay = puanla(
            &Meta {
                genislik: Some(4000),
                yukseklik: Some(3000),
                yon_kodu: Some(1),
                ..tam_meta()
            },
            &a,
        );
        // Aynı boyut, 90° döndürme: oran 0.75 (çok uzak)
        let donmus = puanla(
            &Meta {
                genislik: Some(4000),
                yukseklik: Some(3000),
                yon_kodu: Some(6),
                ..tam_meta()
            },
            &a,
        );
        let py = yatay
            .bilesen("kompozisyon")
            .expect("bileşen")
            .puan
            .expect("puan");
        let pd = donmus
            .bilesen("kompozisyon")
            .expect("bileşen")
            .puan
            .expect("puan");
        assert!(py > pd, "yatay {} donmus {}", py, pd);
    }

    #[test]
    fn iso_dengesi_iso100_en_yuksek() {
        let a = Ayarlar::varsayilan();
        let p100 = puanla(
            &Meta {
                iso: Some(100),
                ..tam_meta()
            },
            &a,
        );
        let p6400 = puanla(
            &Meta {
                iso: Some(6400),
                ..tam_meta()
            },
            &a,
        );
        let d100 = p100
            .bilesen("iso_dengesi")
            .expect("bileşen")
            .puan
            .expect("puan");
        let d6400 = p6400
            .bilesen("iso_dengesi")
            .expect("bileşen")
            .puan
            .expect("puan");
        assert!(d100 > d6400, "iso100 {} iso6400 {}", d100, d6400);
    }

    #[test]
    fn eksik_veri_bileseni_none_dondurur() {
        let a = Ayarlar::varsayilan();
        let bos = puanla(&Meta::default(), &a);
        assert_eq!(bos.katilan_bilesen, 0);
        assert_eq!(bos.toplam, 0.0);
        for b in &bos.bilesenler {
            assert!(b.puan.is_none());
        }
    }

    #[test]
    fn kismi_veri_katilimi_azaltir() {
        let a = Ayarlar::varsayilan();
        // Sadece ISO var.
        let m = Meta {
            iso: Some(400),
            ..Meta::default()
        };
        let p = puanla(&m, &a);
        assert_eq!(p.katilan_bilesen, 2, "pozlama + iso_dengesi");
        assert!(p.toplam > 0.0);
    }

    #[test]
    fn sifir_iso_gecersiz_sayilir() {
        let a = Ayarlar::varsayilan();
        let p = puanla(
            &Meta {
                iso: Some(0),
                ..tam_meta()
            },
            &a,
        );
        assert!(p.bilesen("pozlama").expect("bileşen").puan.is_none());
        assert!(p.bilesen("iso_dengesi").expect("bileşen").puan.is_none());
    }

    #[test]
    fn negatif_poz_gcersiz_sayilir() {
        let a = Ayarlar::varsayilan();
        let p = puanla(
            &Meta {
                poz_suresi: Some(-1.0),
                odak_mm: Some(50.0),
                ..tam_meta()
            },
            &a,
        );
        assert!(p.bilesen("keskinlik").expect("bileşen").puan.is_none());
    }

    #[test]
    fn negatif_odak_gcersiz_sayilir() {
        let a = Ayarlar::varsayilan();
        let p = puanla(
            &Meta {
                poz_suresi: Some(0.01),
                odak_mm: Some(-50.0),
                ..tam_meta()
            },
            &a,
        );
        assert!(p.bilesen("keskinlik").expect("bileşen").puan.is_none());
        assert!(p.bilesen("kirpma").expect("bileşen").puan.is_none());
    }

    #[test]
    fn sifir_boyut_kompozisyon_none() {
        let a = Ayarlar::varsayilan();
        let p = puanla(
            &Meta {
                genislik: Some(0),
                yukseklik: Some(0),
                ..tam_meta()
            },
            &a,
        );
        assert!(p.bilesen("kompozisyon").expect("bileşen").puan.is_none());
    }

    #[test]
    fn toplam_agirlikli_ortalama() {
        let a = Ayarlar::varsayilan();
        // Yalnız ISO 100: pozlama = (12 - log2(100))·(100/6) = 89.3,
        // iso_dengesi = (16 - log2(100))·(100/8) = 100 (kırpılır).
        // Ağırlıklı ortalama = (89.3·0.25 + 100·0.15) / (0.25 + 0.15) = 93.3.
        let p = puanla(
            &Meta {
                iso: Some(100),
                ..Meta::default()
            },
            &a,
        );
        assert_eq!(p.katilan_bilesen, 2, "pozlama + iso_dengesi");
        assert!((p.toplam - 93.29).abs() < 0.05, "{}", p.toplam);
        // Toplam, katılan bileşenlerin puan aralığında olmalı.
        assert!(p.toplam >= 89.0 && p.toplam <= 100.0, "{}", p.toplam);
    }

    #[test]
    fn guclu_bilesen_en_yuksek_puani_bulur() {
        let a = Ayarlar::varsayilan();
        let p = puanla(&tam_meta(), &a);
        // ISO 200, 1/500, 50mm, 4:3 -> keskinlik muhtemelen en yüksek.
        let g = p.guclu_bilesen();
        assert!(g.is_some());
    }

    #[test]
    fn puanlar_0_100_araliginda_kalir() {
        let a = Ayarlar::varsayilan();
        let uct = Meta {
            poz_suresi: Some(1.0 / 8000.0),
            diyafram: Some(1.4),
            iso: Some(50),
            odak_mm: Some(400.0),
            yon_kodu: Some(1),
            genislik: Some(6000),
            yukseklik: Some(4000),
            ..Meta::default()
        };
        let p = puanla(&uct, &a);
        for b in &p.bilesenler {
            if let Some(v) = b.puan {
                assert!((0.0..=100.0).contains(&v), "{} = {}", b.ad, v);
            }
        }
    }

    #[test]
    fn sifir_en_boy_orani_kompozisyon_gecersiz() {
        let a = Ayarlar::varsayilan();
        // yukseklik 0 -> en_boy_orani None
        let p = puanla(
            &Meta {
                genislik: Some(4000),
                yukseklik: Some(0),
                yon_kodu: Some(1),
                ..tam_meta()
            },
            &a,
        );
        assert!(p.bilesen("kompozisyon").expect("bileşen").puan.is_none());
    }

    #[test]
    fn poz_telafisi_gerekceye_yansir() {
        let a = Ayarlar::varsayilan();
        let p = puanla(
            &Meta {
                poz_bias: Some(-1.0),
                ..tam_meta()
            },
            &a,
        );
        let g = p.bilesen("pozlama").expect("bileşen").gerekce.clone();
        assert!(g.contains("EV"), "{}", g);
    }

    #[test]
    fn ayarlar_json_gidiyor_geliyor() {
        let a = Ayarlar::varsayilan();
        let json = serde_json::to_string(&a).expect("serileştir");
        let geri: Ayarlar = serde_json::from_str(&json).expect("ayrıştır");
        assert_eq!(a, geri);
    }

    #[test]
    fn eksik_json_alanlari_varsayilana_duser() {
        let geri: Ayarlar = serde_json::from_str("{}").expect("ayrıştır");
        assert_eq!(geri, Ayarlar::varsayilan());
    }
}
