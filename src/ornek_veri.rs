//! Örnek dosya üreticisi — geçerli TIFF/EXIF dosyaları ve gömülü JPEG önizlemeleri.
//!
//! # Neden var
//!
//! Ortamda gerçek `.CR2`/`.NEF` dosyası yoktur ve `image`/`tiff` crate'leri
//! bağımlılık politikası nedeniyle yasaktır (WORKER_CONTRACT § 3.2-F).
//! Testlerin gerçek bayt dizileriyle çalışması için Exif 2.3 ve TIFF 6.0
//! kurallarına uyan küçük bir üretici yazıldı.
//!
//! # Dosya yerleşimi (iki geçişli hesaplama)
//!
//! ```text
//! 0x00  "II" 0x2A 0x00 + IFD0 offset (8)
//! 0x08  IFD0     : 2 + n*12 + 4 bayt
//!        Exif IFD: 2 + m*12 + 4 bayt
//!        değer   : ASCII metinler ve RATIONAL çiftleri (4 bayttan büyükler)
//!        önizleme: gömülü JPEG bayt dizisi
//! ```
//!
//! Değer bloklarının mutlak offset'leri dosya düzenine bağlıdır; bu yüzden
//! önce boyutlar hesaplanır, sonra offset'ler yazılır.
//!
//! Kaynak: TIFF 6.0 Section 2, Exif 2.3 Section 4.

use crate::jpeg_test_veri;

/// Üretilecek tek bir örnek dosyanın nitelikleri.
#[derive(Debug, Clone)]
pub struct Kayit {
    /// Kamera üreticisi (Make, 0x010F).
    pub make: String,
    /// Kamera modeli (Model, 0x0110).
    pub model: String,
    /// Çekim zamanı (`DateTimeOriginal`, 0x9003), `YYYY:MM:DD HH:MM:SS`.
    pub zaman: Option<String>,
    /// `ExposureTime` (0x829A), saniye.
    pub poz_suresi: Option<f32>,
    /// `FNumber` (0x829D).
    pub diyafram: Option<f32>,
    /// `ISOSpeedRatings` (0x8827).
    pub iso: Option<u32>,
    /// `FocalLength` (0x920A), mm.
    pub odak_mm: Option<f32>,
    /// `Orientation` (0x0112), 1-8.
    pub yon_kodu: Option<u16>,
    /// `PixelXDimension` (0xA002) ve `ImageWidth` (0x0100) için genişlik.
    pub genislik: Option<u32>,
    /// `PixelYDimension` (0xA003) ve `ImageLength` (0x0101) için yükseklik.
    pub yukseklik: Option<u32>,
    /// Gömülü önizleme: (genişlik, yükseklik, gri seviyesi).
    pub onizleme: Option<(u16, u16, u8)>,
    /// Gömülü önizleme deseni (dolu ise `onizleme` alanı yok sayılır).
    ///
    /// Düz renkler algısal hash üretmez (tüm AC katsayıları sıfırdır); gerçekçi
    /// gruplama testleri için desenli önizlemeler gerekir.
    pub onizleme_deseni: Option<crate::jpeg_test_veri::Desen>,
    /// Önizleme baytları geçersiz kılınacaksa `true`.
    pub onizlemeyi_boz: bool,
}

impl Default for Kayit {
    fn default() -> Self {
        Kayit {
            make: "Canon".to_string(),
            model: "Test Kamera".to_string(),
            zaman: Some("2026:09:29 10:30:00".to_string()),
            poz_suresi: Some(1.0 / 500.0),
            diyafram: Some(2.8),
            iso: Some(200),
            odak_mm: Some(50.0),
            yon_kodu: Some(1),
            genislik: Some(4000),
            yukseklik: Some(3000),
            onizleme: None,
            onizleme_deseni: None,
            onizlemeyi_boz: false,
        }
    }
}

impl Kayit {
    /// Tipik Canon burst karesi.
    pub fn canon() -> Kayit {
        Kayit::default()
    }

    /// Nikon gövdesi, farklı metadata.
    pub fn nikon() -> Kayit {
        Kayit {
            make: "NIKON CORPORATION".to_string(),
            model: "NIKON Z 6_2".to_string(),
            zaman: Some("2026:08:14 18:05:33".to_string()),
            poz_suresi: Some(1.0 / 1000.0),
            diyafram: Some(4.0),
            iso: Some(400),
            odak_mm: Some(85.0),
            yon_kodu: Some(1),
            genislik: Some(6048),
            yukseklik: Some(4024),
            onizleme: None,
            onizleme_deseni: None,
            onizlemeyi_boz: false,
        }
    }

    /// Sony gövdesi, 90° döndürülmüş yön kodu.
    pub fn sony() -> Kayit {
        Kayit {
            make: "SONY".to_string(),
            model: "ILCE-7M4".to_string(),
            zaman: Some("2026:07:01 09:12:00".to_string()),
            poz_suresi: Some(1.0 / 250.0),
            diyafram: Some(5.6),
            iso: Some(100),
            odak_mm: Some(35.0),
            yon_kodu: Some(6),
            genislik: Some(7008),
            yukseklik: Some(4672),
            onizleme: None,
            onizleme_deseni: None,
            onizlemeyi_boz: false,
        }
    }

    /// Önizlemesiz sıradan kayıt.
    pub fn oran() -> Kayit {
        Kayit::default()
    }

    /// Belirtilen boyut ve gri seviyesiyle gömülü önizleme.
    pub fn onizleme_ile(genislik: u16, yukseklik: u16, gri: u8) -> Kayit {
        Kayit {
            onizleme: Some((genislik, yukseklik, gri)),
            ..Kayit::default()
        }
    }

    /// Desenli önizleme (hash üretir, gerçekçi gruplama testleri için).
    pub fn desenli_onizleme_ile(desen: crate::jpeg_test_veri::Desen) -> Kayit {
        Kayit {
            onizleme_deseni: Some(desen),
            ..Kayit::default()
        }
    }

    /// Baytları geçersiz kılınmış gömülü önizleme.
    pub fn bozuk_onizleme_ile() -> Kayit {
        Kayit {
            onizleme: Some((16, 16, 100)),
            onizlemeyi_boz: true,
            ..Kayit::default()
        }
    }
}

/// Bir IFD girdisi; değer 4 bayttan küçükse satır içi, değilse ayrı blokta.
struct Giris {
    etiket: u16,
    tip: u16,
    adet: u32,
    /// Değerin ham baytları; 4 bayttan küçükse satır içine yazılır.
    veri: Vec<u8>,
    /// Değer 4 bayttan büyükse değer alanına yazılacak mutlak offset.
    deger_ofseti: u32,
}

impl Giris {
    fn short(etiket: u16, v: u16) -> Giris {
        Giris {
            etiket,
            tip: 3,
            adet: 1,
            veri: v.to_le_bytes().to_vec(),
            deger_ofseti: 0,
        }
    }

    fn long(etiket: u16, v: u32) -> Giris {
        Giris {
            etiket,
            tip: 4,
            adet: 1,
            veri: v.to_le_bytes().to_vec(),
            deger_ofseti: 0,
        }
    }

    fn ascii(etiket: u16, metin: &str) -> Giris {
        let mut b = metin.as_bytes().to_vec();
        b.push(0);
        let adet = b.len() as u32;
        Giris {
            etiket,
            tip: 2,
            adet,
            veri: b,
            deger_ofseti: 0,
        }
    }

    /// RATIONAL: iki `u32` (pay, payda).
    fn rasyonel(etiket: u16, pay: u32, payda: u32) -> Giris {
        let mut b = Vec::with_capacity(8);
        b.extend_from_slice(&pay.to_le_bytes());
        b.extend_from_slice(&payda.to_le_bytes());
        Giris {
            etiket,
            tip: 5,
            adet: 1,
            veri: b,
            deger_ofseti: 0,
        }
    }

    /// Sıfır uzunluklu yer tutucu (offset sonradan doldurulacak girdiler için).
    fn yer_tutucu(etiket: u16, tip: u16, adet: u32) -> Giris {
        Giris {
            etiket,
            tip,
            adet,
            veri: vec![0u8; (tip_genislik(tip) * adet as usize).clamp(1, 4)],
            deger_ofseti: 0,
        }
    }

    fn satir_ici_mi(&self) -> bool {
        self.veri.len() <= 4
    }
}

/// TIFF alan tipinin bayt genişliği.
fn tip_genislik(tip: u16) -> usize {
    match tip {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 => 8,
        _ => 1,
    }
}

/// Bir dizinin bayt uzunluğu (girdi sayısı + sonraki IFD offset'i).
fn dizin_boyutu(girdiler: &[Giris]) -> usize {
    2 + girdiler.len() * 12 + 4
}

/// Bir dizinin dış değer bloklarının toplam bayt uzunluğu (hizalama düzeltmesiyle).
fn dever_boyutu(girdiler: &[Giris]) -> usize {
    girdiler
        .iter()
        .filter(|g| !g.satir_ici_mi())
        .map(|g| {
            let b = g.veri.len();
            b + (b % 2)
        })
        .sum()
}

/// TIFF/EXIF dosyası üretir (tek kayıt).
pub fn tif_tek(kayit: &Kayit) -> Vec<u8> {
    tif_ornegi(std::slice::from_ref(kayit))
}

/// Verilen kayıtlardan TIFF/EXIF dosyası üretir.
///
/// Çok kayıt verilirse yalnız ilki kullanılır; kalanları yok sayılır
/// (bu bir test üreticisidir, çok dosyalı bir RAW değildir).
pub fn tif_ornegi(kayitlar: &[Kayit]) -> Vec<u8> {
    let k = &kayitlar[0];
    /// Desenli önizleme için sabit önizleme boyutu (piksel).
    const DESEN_BOYUTU: u32 = 64;
    let jpeg = if let Some(desen) = k.onizleme_deseni {
        Some(jpeg_test_veri::desen_jpeg(
            desen,
            DESEN_BOYUTU,
            DESEN_BOYUTU,
        ))
    } else {
        k.onizleme.map(|(g, y, gri)| {
            if k.onizlemeyi_boz {
                vec![0xFF, 0xD8, 0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11]
            } else {
                jpeg_test_veri::tek_duz_blok(g, y, gri)
            }
        })
    };

    // 1. geçiş: IFD0 ve Exif girdilerini kur, Exif pointer yer tutucusu ekle.
    let mut ifd0: Vec<Giris> = vec![Giris::ascii(0x010F, &k.make)];
    if let Some(w) = k.genislik {
        ifd0.push(Giris::long(0x0100, w));
    }
    if let Some(h) = k.yukseklik {
        ifd0.push(Giris::long(0x0101, h));
    }
    if let Some(y) = k.yon_kodu {
        ifd0.push(Giris::short(0x0112, y));
    }
    ifd0.push(Giris::ascii(0x0110, &k.model));
    let exif_pointer_poz = ifd0.len();
    ifd0.push(Giris::yer_tutucu(0x8769, 4, 1));

    let mut exif: Vec<Giris> = Vec::new();
    if let Some(z) = &k.zaman {
        exif.push(Giris::ascii(0x9003, z));
    }
    if let Some(p) = k.poz_suresi {
        exif.push(Giris::rasyonel(0x829A, (p * 1_000_000.0) as u32, 1_000_000));
    }
    if let Some(d) = k.diyafram {
        exif.push(Giris::rasyonel(0x829D, (d * 10.0) as u32, 10));
    }
    if let Some(i) = k.iso {
        exif.push(Giris::short(0x8827, i as u16));
    }
    if let Some(o) = k.odak_mm {
        exif.push(Giris::rasyonel(0x920A, (o * 10.0) as u32, 10));
    }
    if let Some(w) = k.genislik {
        exif.push(Giris::long(0xA002, w));
    }
    if let Some(h) = k.yukseklik {
        exif.push(Giris::long(0xA003, h));
    }
    let jpeg_uzunluk = jpeg.as_ref().map(std::vec::Vec::len).unwrap_or(0);
    let jpeg_poz = if jpeg.is_some() {
        exif.push(Giris::yer_tutucu(0x0201, 4, 1));
        exif.push(Giris::yer_tutucu(0x0202, 4, 1));
        Some(exif.len() - 2)
    } else {
        None
    };

    // 2. geçiş: mutlak offset'leri hesapla ve satır dışı değer alanlarına yaz.
    let ifd0_ofset = 8u64;
    let exif_ofset = ifd0_ofset + dizin_boyutu(&ifd0) as u64;
    let deger_ofset = exif_ofset + dizin_boyutu(&exif) as u64;
    let jpeg_ofset = deger_ofset + dever_boyutu(&ifd0) as u64 + dever_boyutu(&exif) as u64;

    ifd0[exif_pointer_poz].veri = (exif_ofset as u32).to_le_bytes().to_vec();
    if let Some(p) = jpeg_poz {
        exif[p].veri = (jpeg_ofset as u32).to_le_bytes().to_vec();
        exif[p + 1].veri = (jpeg_uzunluk as u32).to_le_bytes().to_vec();
    }
    // Satır dışı değerlerin offset'leri veri bloğunun başından itibaren
    // hesaplanır; TIFF'te offset'ler dosya başından mutlaktır.
    deger_offsetlerini_doldur(&mut ifd0, deger_ofset);
    deger_offsetlerini_doldur(&mut exif, deger_ofset + dever_boyutu(&ifd0) as u64);

    // 3. geçiş: dosyayı yaz.
    let mut v: Vec<u8> = Vec::new();
    v.extend_from_slice(b"II\x2a\x00");
    v.extend_from_slice(&(ifd0_ofset as u32).to_le_bytes());
    dizin_yaz(&mut v, &ifd0);
    dizin_yaz(&mut v, &exif);
    deger_yaz(&mut v, &ifd0);
    deger_yaz(&mut v, &exif);
    if let Some(j) = jpeg {
        v.extend_from_slice(&j);
    }
    v
}

/// Satır dışı girdilerin değer alanına mutlak offset yazar.
fn deger_offsetlerini_doldur(girdiler: &mut [Giris], baslangic: u64) {
    let mut ofset = baslangic;
    for g in girdiler.iter_mut() {
        if g.satir_ici_mi() {
            continue;
        }
        g.deger_ofseti = ofset as u32;
        ofset += g.veri.len() as u64;
        ofset += g.veri.len() as u64 % 2;
    }
}

/// Bir IFD dizinini (girdi sayısı + girdiler + sonraki offset 0) yazar.
fn dizin_yaz(v: &mut Vec<u8>, girdiler: &[Giris]) {
    v.extend_from_slice(&(girdiler.len() as u16).to_le_bytes());
    for g in girdiler {
        v.extend_from_slice(&g.etiket.to_le_bytes());
        v.extend_from_slice(&g.tip.to_le_bytes());
        v.extend_from_slice(&g.adet.to_le_bytes());
        if g.satir_ici_mi() {
            let mut alan = [0u8; 4];
            alan[..g.veri.len()].copy_from_slice(&g.veri);
            v.extend_from_slice(&alan);
        } else {
            v.extend_from_slice(&g.deger_ofseti.to_le_bytes());
        }
    }
    v.extend_from_slice(&0u32.to_le_bytes());
}

/// Satır dışı değer bloklarını sırayla yazar.
fn deger_yaz(v: &mut Vec<u8>, girdiler: &[Giris]) {
    for g in girdiler {
        if g.satir_ici_mi() {
            continue;
        }
        v.extend_from_slice(&g.veri);
        if g.veri.len() % 2 == 1 {
            v.push(0); // TIFF sözcük hizası
        }
    }
}

#[cfg(test)]
// Gerekçe: `unwrap`/`expect` yalnızca test içinde kullanılır.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::tiff;

    #[test]
    fn tif_dosyasi_gecerli_baslik_icerir() {
        let v = tif_tek(&Kayit::oran());
        assert_eq!(&v[0..4], b"II\x2a\x00");
        assert!(v.len() > 100, "{}", v.len());
    }

    #[test]
    fn tif_dosyasi_ayristirilabilir() {
        let v = tif_tek(&Kayit::canon());
        let f = jpeg_test_veri::GeciciDosya::yeni("ornek", &v).expect("yaz");
        let belge = tiff::coz(f.yol()).expect("coz");
        assert!(belge.exif.is_some(), "Exif IFD çözülmeli");
    }

    #[test]
    fn kamera_alanlari_okunur() {
        let v = tif_tek(&Kayit::canon());
        let f = jpeg_test_veri::GeciciDosya::yeni("kamera", &v).expect("yaz");
        let (mut okuyucu, baslik) = tiff::TiffOkuyucu::ac(f.yol()).expect("ac");
        let mut ziyaret = std::collections::BTreeSet::new();
        let ifd0 = okuyucu
            .ifd_oku(u64::from(baslik.ifd0_ofseti), &mut ziyaret, 1)
            .expect("ifd0");
        let make = ifd0.bul(0x010F).expect("make");
        assert_eq!(
            okuyucu.deger_metin(make).expect("metin").as_deref(),
            Some("Canon")
        );
    }
}
