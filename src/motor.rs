//! Tarama motoru — dosya → metadatası → hash → puan zinciri.
//!
//! # Akış
//!
//! 1. [`crate::gezgin`] ile desteklenen dosyalar bulunur.
//! 2. Her dosya **tek tek** açılır, IFD dizini çözülür, metadatası okunur,
//!    dosya **kapatılır**.
//! 3. Gömülü önizleme bulunursa yalnız o aralık okunur, JPEG çözülür ve
//!    algısal hash hesaplanır.
//! 4. Puanlanır, sonuç bellekte **yalnız küçük bir kayıt** olarak kalır.
//!
//! # Bellek sözleşmesi (rapor `b08`)
//!
//! - Görüntü pikselleri **hiçbir zaman saklanmaz**; `KareKaydi` yalnız
//!   hash, puan, boyut ve yol tutar.
//! - Tam çözüm yoktur; piksel verisi dosyadan asla tümüyle okunmaz.
//! - Her dosya kapanırken `TiffOkuyucu` düşer; tepe bellek dosya sayısıyla
//!   artmaz.
//! - Önizleme baytları için sert üst sınır: [`ONIZLEME_BELLEK_LIMITI`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::bhash::{hash_goruntuden, AlgisalHash};
use crate::gezgin;
use crate::hata::Sonuc;
use crate::jpeg::coz_jpeg;
use crate::meta::{coz_belge, Meta};
use crate::onizleme::{onizleme_baytlarini_oku, onizleme_konumu};
use crate::puan::{puanla, Ayarlar, Puan};
use crate::tiff::{self, TiffOkuyucu};

/// Tek bir dosyanın önizlemesi belleğe alınabilecek en fazla baytı (24 MiB).
///
/// Kamera gömülü önizlemeleri tipik olarak 0,5-4 MB'dir; bu sınır bozuk bir
/// etiketin devasa tahsis yaptırmasını engeller.
pub const ONIZLEME_BELLEK_LIMITI: usize = 24 * 1024 * 1024;

/// Taranan tek bir dosyanın kaydı.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KareKaydi {
    /// Dosyanın tam yolu.
    pub yol: PathBuf,
    /// Dosya adı (rapor okunurluğu için).
    pub dosya_adi: String,
    /// Dosyanın diskteki bayt uzunluğu.
    pub dosya_boyutu: u64,
    /// Tarama sırasında **gerçekten okunan** bayt (kısmi okuma kanıtı).
    pub okunan_bayt: u64,
    /// Algısal hash; önizleme yoksa `None`.
    pub hash: Option<AlgisalHashSeri>,
    /// Gömülü önizleme bulundu mu?
    pub onizleme_var: bool,
    /// Önizleme bulunduysa bayt uzunluğu.
    pub onizleme_bayt: Option<u64>,
    /// Çözülmüş metadatası.
    pub meta: MetaOzet,
    /// Beş bileşenli puan.
    pub puan: Puan,
}

/// Hash'in JSON'a yazılabilir biçimi (16 ondalık hane).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AlgisalHashSeri(pub u64);

impl From<AlgisalHash> for AlgisalHashSeri {
    fn from(h: AlgisalHash) -> Self {
        AlgisalHashSeri(h.0)
    }
}

impl AlgisalHashSeri {
    /// 16 ondalık haneye çevirir.
    pub fn onaltilik(&self) -> String {
        format!("{:016x}", self.0)
    }
}

/// Raporun JSON şemasına sığan metadatası özeti.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetaOzet {
    /// Kamera üreticisi.
    pub make: Option<String>,
    /// Kamera modeli.
    pub model: Option<String>,
    /// Çekim zamanı (ISO 8601).
    pub zaman: Option<String>,
    /// Poz süresi (saniye).
    pub poz_suresi: Option<f32>,
    /// Diyafram.
    pub diyafram: Option<f32>,
    /// ISO.
    pub iso: Option<u32>,
    /// Odak uzaklığı (mm).
    pub odak_mm: Option<f32>,
    /// Yön kodu.
    pub yon_kodu: Option<u16>,
    /// Genişlik (piksel).
    pub genislik: Option<u32>,
    /// Yükseklik (piksel).
    pub yukseklik: Option<u32>,
    /// Yöne göre düzeltilmiş en-boy oranı.
    pub en_boy_orani: Option<f64>,
    /// Tahmini crop faktörü.
    pub crop_faktor: f64,
}

impl From<&Meta> for MetaOzet {
    fn from(m: &Meta) -> Self {
        MetaOzet {
            make: m.make.clone(),
            model: m.model.clone(),
            zaman: m.zaman.map(|z| z.iso8601()),
            poz_suresi: m.poz_suresi,
            diyafram: m.diyafram,
            iso: m.iso,
            odak_mm: m.odak_mm,
            yon_kodu: m.yon_kodu,
            genislik: m.genislik,
            yukseklik: m.yukseklik,
            en_boy_orani: m.en_boy_orani(),
            crop_faktor: m.crop_faktor(),
        }
    }
}

/// Okunamayan/bozuk dosya kaydı — taramayı durdurmaz.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtlananDosya {
    /// Dosyanın yolu.
    pub yol: PathBuf,
    /// Hata etiketi (`tiff-bazli-degil`, `bigtiff`, `ifd-gecersiz` ...).
    pub hata: String,
    /// Kısa açıklama.
    pub ayrinti: String,
}

impl AtlananDosya {
    /// `Hata` değerinden kayıt üretir.
    pub fn hatadan(yol: &Path, hata: &crate::hata::Hata) -> Self {
        AtlananDosya {
            yol: yol.to_path_buf(),
            hata: hata.etiket().to_string(),
            ayrinti: hata.to_string(),
        }
    }
}

/// Tarama istatistiği — "kısmi okuma" iddiasının sayısal kanıtı.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Istatistik {
    /// Gezginin bulduğu desteklenen dosya sayısı.
    pub bulunan_dosya: usize,
    /// Başarıyla metadatası okunan dosya sayısı.
    pub meta_okunan: usize,
    /// Gömülü önizlemesi bulunan dosya sayısı.
    pub onizleme_bulunan: usize,
    /// Önizlemesi bulunmayan dosya sayısı.
    pub onizleme_yok: usize,
    /// Önizlemesi bulunan ama **çözülemeyen** dosya sayısı.
    pub onizleme_bozuk: usize,
    /// Algısal hash üretilen dosya sayısı.
    pub hash_uretilen: usize,
    /// Atlanan (okunamayan/bozuk) dosya sayısı.
    pub atlanan: usize,
    /// Toplam **okunan** bayt.
    pub toplam_okunan_bayt: u64,
    /// Dosyaların toplam diskteki boyutu.
    pub toplam_dosya_boyutu: u64,
}

impl Istatistik {
    /// Okunan baytların toplam dosya boyutuna oranı (0-1).
    ///
    /// Bu değer ne kadar küçükse tarama o kadar "kısmi"dır. 3000 dosyalık
    /// bir klasörde tipik olarak %1'in altında kalır.
    pub fn okuma_orani(&self) -> f64 {
        if self.toplam_dosya_boyutu == 0 {
            return 0.0;
        }
        self.toplam_okunan_bayt as f64 / self.toplam_dosya_boyutu as f64
    }
}

/// Tarama motoru — ayarlarla çalışır.
#[derive(Debug, Clone)]
pub struct Motor {
    /// Puanlama ağırlıkları.
    pub ayarlar: Ayarlar,
    /// Hash üretilsin mi?
    pub hash_uret: bool,
}

impl Default for Motor {
    fn default() -> Self {
        Motor {
            ayarlar: Ayarlar::varsayilan(),
            hash_uret: true,
        }
    }
}

impl Motor {
    /// Varsayılan ayarlarla motor oluşturur.
    pub fn yeni() -> Self {
        Motor::default()
    }

    /// Kök dizini tarar ve tüm kayıtları üretir.
    ///
    /// Bozuk dosyalar `AtlananDosya` olarak toplanır; taramayı durdurmaz.
    pub fn tara(&self, kok: &Path) -> Sonuc<Vec<KareKaydi>> {
        let (kayitlar, _) = self.tara_ayrinti(kok)?;
        Ok(kayitlar)
    }

    /// Tarar ve atlanan dosyaları da döndürür.
    pub fn tara_ayrinti(&self, kok: &Path) -> Sonuc<(Vec<KareKaydi>, Vec<AtlananDosya>)> {
        let gezinme = gezgin::gez(kok)?;
        let mut kayitlar = Vec::with_capacity(gezinme.dosyalar.len());
        let mut atlanan = Vec::new();
        for yol in &gezinme.dosyalar {
            match self.dosya_isle(yol) {
                Ok(kayit) => kayitlar.push(kayit),
                Err(hata) => atlanan.push(AtlananDosya::hatadan(yol, &hata)),
            }
        }
        Ok((kayitlar, atlanan))
    }

    /// Tek bir dosyayı açar, metadatasını çözer, gerekiyorsa hash'ler.
    ///
    /// Dosya bu fonksiyon sonunda **kapanır**: `TiffOkuyucu` yereldir.
    pub fn dosya_isle(&self, yol: &Path) -> Sonuc<KareKaydi> {
        let (mut okuyucu, _baslik) = TiffOkuyucu::ac(yol)?;
        let belge = tiff::coz_oku(&mut okuyucu)?;
        let meta = coz_belge(&mut okuyucu, &belge)?;

        // Önizleme: yalnız bulunursa okunur ve çözülür.
        let mut hash = None;
        let mut onizleme_var = false;
        let mut onizleme_bayt = None;
        if self.hash_uret {
            if let Ok(konum) = onizleme_konumu(&mut okuyucu, &belge) {
                onizleme_var = true;
                onizleme_bayt = Some(konum.uzunluk);
                if let Ok(baytlar) =
                    onizleme_baytlarini_oku(&mut okuyucu, &konum, ONIZLEME_BELLEK_LIMITI)
                {
                    if let Ok(goruntu) = coz_jpeg(&baytlar) {
                        hash = hash_goruntuden(&goruntu).map(AlgisalHashSeri::from);
                    }
                }
            }
        }

        let son_okunan = okuyucu.okunan_bayt();
        let puan = puanla(&meta, &self.ayarlar);
        Ok(KareKaydi {
            yol: yol.to_path_buf(),
            dosya_adi: yol
                .file_name()
                .map(|a| a.to_string_lossy().to_string())
                .unwrap_or_default(),
            dosya_boyutu: belge.dosya_boyutu,
            okunan_bayt: son_okunan,
            hash,
            onizleme_var,
            onizleme_bayt,
            meta: MetaOzet::from(&meta),
            puan,
        })
    }
}

/// Kayıtlardan tarama istatistiği üretir.
pub fn istatistik(kayitlar: &[KareKaydi], atlanan: &[AtlananDosya]) -> Istatistik {
    let mut i = Istatistik {
        bulunan_dosya: kayitlar.len() + atlanan.len(),
        atlanan: atlanan.len(),
        ..Istatistik::default()
    };
    for k in kayitlar {
        i.meta_okunan += 1;
        if k.onizleme_var {
            i.onizleme_bulunan += 1;
            if k.hash.is_some() {
                i.hash_uretilen += 1;
            } else {
                i.onizleme_bozuk += 1;
            }
        } else {
            i.onizleme_yok += 1;
        }
        i.toplam_okunan_bayt += k.okunan_bayt;
        i.toplam_dosya_boyutu += k.dosya_boyutu;
    }
    i
}

#[cfg(test)]
// Gerekçe: `unwrap`/`expect` yalnızca test içinde kullanılır.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::ornek_veri;

    struct GeciciDizin {
        yol: PathBuf,
    }

    impl GeciciDizin {
        fn yeni(etiket: &str) -> GeciciDizin {
            let yol =
                std::env::temp_dir().join(format!("bj-motor-{}-{}", etiket, std::process::id()));
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
    fn bos_klasor_bos_kayit_dondurur() {
        let d = GeciciDizin::yeni("bos");
        let m = Motor::yeni();
        let k = m.tara(&d.yol).expect("tara");
        assert!(k.is_empty());
    }

    #[test]
    fn tek_dosya_okunur() {
        let d = GeciciDizin::yeni("tek");
        let veri = ornek_veri::tif_ornegi(&[ornek_veri::Kayit::oran()]);
        std::fs::write(d.yol.join("a.cr2"), &veri).expect("yaz");
        let m = Motor::yeni();
        let k = m.tara(&d.yol).expect("tara");
        assert_eq!(k.len(), 1);
        assert_eq!(k[0].dosya_adi, "a.cr2");
        assert_eq!(k[0].meta.model.as_deref(), Some("Test Kamera"));
    }

    #[test]
    fn bozuk_dosya_atlanir_tarama_devam_eder() {
        let d = GeciciDizin::yeni("bozuk");
        std::fs::write(d.yol.join("bozuk.cr2"), b"bu bir tiff degil").expect("yaz");
        let veri = ornek_veri::tif_ornegi(&[ornek_veri::Kayit::oran()]);
        std::fs::write(d.yol.join("iyi.cr2"), &veri).expect("yaz");
        let m = Motor::yeni();
        let (k, a) = m.tara_ayrinti(&d.yol).expect("tara");
        assert_eq!(k.len(), 1, "iyi dosya okunmalı");
        assert_eq!(a.len(), 1, "bozuk dosya atlanmalı");
        assert_eq!(a[0].hata, "tiff-bazli-degil");
    }

    #[test]
    fn bigtiff_dosyasi_atlanir() {
        let d = GeciciDizin::yeni("bigtiff");
        let mut v = Vec::from(*b"II\x2b\x00");
        v.extend_from_slice(&8u32.to_le_bytes());
        v.resize(64, 0);
        std::fs::write(d.yol.join("a.dng"), &v).expect("yaz");
        let m = Motor::yeni();
        let (_, a) = m.tara_ayrinti(&d.yol).expect("tara");
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].hata, "bigtiff");
    }

    #[test]
    fn onizleme_bulunan_dosyada_hash_uretilir() {
        let d = GeciciDizin::yeni("onizleme");
        let veri = ornek_veri::tif_ornegi(&[ornek_veri::Kayit::onizleme_ile(16, 16, 90)]);
        std::fs::write(d.yol.join("a.cr2"), &veri).expect("yaz");
        let m = Motor::yeni();
        let k = m.tara(&d.yol).expect("tara");
        assert_eq!(k.len(), 1);
        assert!(k[0].onizleme_var);
        assert!(k[0].hash.is_some(), "hash üretilmeliydi");
    }

    #[test]
    fn onizlemesiz_dosyada_hash_yoktur() {
        let d = GeciciDizin::yeni("onizlemesiz");
        let veri = ornek_veri::tif_ornegi(&[ornek_veri::Kayit::oran()]);
        std::fs::write(d.yol.join("a.cr2"), &veri).expect("yaz");
        let m = Motor::yeni();
        let k = m.tara(&d.yol).expect("tara");
        assert!(!k[0].onizleme_var);
        assert!(k[0].hash.is_none());
    }

    #[test]
    fn bozuk_onizleme_hash_uretmez_ama_dosya_okunur() {
        let d = GeciciDizin::yeni("bozukonizleme");
        let veri = ornek_veri::tif_ornegi(&[ornek_veri::Kayit::bozuk_onizleme_ile()]);
        std::fs::write(d.yol.join("a.cr2"), &veri).expect("yaz");
        let m = Motor::yeni();
        let (k, a) = m.tara_ayrinti(&d.yol).expect("tara");
        assert_eq!(a.len(), 0, "bozuk önizleme dosyayı atlatmamalı");
        assert_eq!(k.len(), 1);
        assert!(k[0].onizleme_var);
        assert!(k[0].hash.is_none());
    }

    #[test]
    fn onizleme_kapatilirsa_hash_uretilmez() {
        let d = GeciciDizin::yeni("hashkapali");
        let veri = ornek_veri::tif_ornegi(&[ornek_veri::Kayit::onizleme_ile(16, 16, 90)]);
        std::fs::write(d.yol.join("a.cr2"), &veri).expect("yaz");
        let m = Motor {
            ayarlar: Ayarlar::varsayilan(),
            hash_uret: false,
        };
        let k = m.tara(&d.yol).expect("tara");
        assert!(!k[0].onizleme_var);
        assert!(k[0].hash.is_none());
        // Meta yine de okunmuş olmalı.
        assert!(k[0].meta.make.is_some());
    }

    #[test]
    fn kismi_okuma_dosya_boyutunun_cok_altinda_kalir() {
        let d = GeciciDizin::yeni("kismi");
        // 2 MiB'lık dosya: yalnız dizin okunur.
        let veri = ornek_veri::tif_ornegi(&[ornek_veri::Kayit::oran()]);
        let mut buyuk = veri.clone();
        buyuk.resize(2 * 1024 * 1024, 0);
        std::fs::write(d.yol.join("a.cr2"), &buyuk).expect("yaz");
        let m = Motor::yeni();
        let k = m.tara(&d.yol).expect("tara");
        assert!(k[0].okunan_bayt < 2048, "{}", k[0].okunan_bayt);
        assert!(
            k[0].okunan_bayt * 100 < k[0].dosya_boyutu,
            "{} / {}",
            k[0].okunan_bayt,
            k[0].dosya_boyutu
        );
    }

    #[test]
    fn istatistik_toplamlari_dogru() {
        let d = GeciciDizin::yeni("ist");
        std::fs::write(d.yol.join("bozuk.cr2"), b"xx").expect("yaz");
        std::fs::write(
            d.yol.join("a.cr2"),
            ornek_veri::tif_ornegi(&[ornek_veri::Kayit::onizleme_ile(16, 16, 90)]),
        )
        .expect("yaz");
        let m = Motor::yeni();
        let (k, a) = m.tara_ayrinti(&d.yol).expect("tara");
        let i = istatistik(&k, &a);
        assert_eq!(i.bulunan_dosya, 2);
        assert_eq!(i.atlanan, 1);
        assert_eq!(i.onizleme_bulunan, 1);
        assert_eq!(i.hash_uretilen, 1);
        assert!(i.toplam_okunan_bayt > 0);
    }

    #[test]
    fn bos_istatistik_orani_sifirdir() {
        let i = istatistik(&[], &[]);
        assert_eq!(i.okuma_orani(), 0.0);
    }

    #[test]
    fn hash_seri_onaltilik_yazar() {
        let h = AlgisalHashSeri(0xABCDEF0123456789);
        assert_eq!(h.onaltilik(), "abcdef0123456789");
    }
}
