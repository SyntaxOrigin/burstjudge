//! EXIF alanlarının anlamsal çözümü: tarih, sayılar, kamera, yön.
//!
//! Bu katman ham IFD girdilerini [`crate::tiff`] modülünden alır ve fotoğraf
//! triyajında **anlamlı** olan alanlara indirger. Bilinmeyen veya bozuk alan
//! `None` olur; taramayı durdurmaz.
//!
//! # Tarih ayrıştırma
//!
//! Exif 2.3 `DateTimeOriginal` alanı `"YYYY:MM:DD HH:MM:SS"` biçimindedir.
//! Ayrıştırıcı sabit biçimi zorlar; iki nokta üst üste, boş alan, ek
//! karakter veya geçersiz ay/gün/saat aralığı varsa `None` döner. Takvim
//! doğrulaması **yapılmaz** (31 Şubat "geçerli" sayılır) çünkü kaynak
//! dosyanın kendisi böyle bir tarih yazmış olabilir ve dosyayı reddetmek
//! kullanıcıya bilgi kaybı verir.

use crate::hata::Sonuc;
use crate::tiff::{
    self, AlanTipi, Girdi, Ifd, TiffBelge, TiffOkuyucu, ETIKET_BEYAZ_DENGES, ETIKET_CEKIM_TARIHI,
    ETIKET_DIYAFRAM, ETIKET_FLAS, ETIKET_FOTOMETRIK, ETIKET_GENISLIK, ETIKET_ISO,
    ETIKET_ODAK_UZAKLIGI, ETIKET_OLCUM_MODU, ETIKET_ORNEK_ADET, ETIKET_PIXEL_X, ETIKET_PIXEL_Y,
    ETIKET_POZ_BIAS, ETIKET_POZ_PROGRAMI, ETIKET_POZ_SURESI, ETIKET_SERIT_UZUNLUK,
    ETIKET_SIKISTIRMA, ETIKET_TARIH, ETIKET_UZUNLUK, ETIKET_YAZILIM, ETIKET_YON,
};

/// Çekim zamanı — takvim hesabı yapılmaz, yalnız alanlar doğrulanır.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CekimZamani {
    /// Yıl (4 hane).
    pub yil: u16,
    /// Ay (1-12 aralığı doğrulanır).
    pub ay: u8,
    /// Gün (1-31 aralığı doğrulanır).
    pub gun: u8,
    /// Saat (0-23 aralığı doğrulanır).
    pub saat: u8,
    /// Dakika (0-59 aralığı doğrulanır).
    pub dakika: u8,
    /// Saniye (0-59 aralığı doğrulanır; 60 artık saniye kabul edilmez).
    pub saniye: u8,
}

impl CekimZamani {
    /// `"YYYY:MM:DD HH:MM:SS"` metnini ayrıştırır.
    ///
    /// Ayrıştırılamayan her girdi `None` döner: boş metin, yanlış ayırıcı,
    /// eksik alan, ek karakter veya aralık dışı bileşen.
    pub fn ayrıştir(metin: &str) -> Option<CekimZamani> {
        // NUL sonlu alanlar ve sonrası sık görülür; ilk NUL'da kes.
        let m = metin.split('\0').next().unwrap_or("").trim();
        if m.len() != 19 {
            return None;
        }
        let b = m.as_bytes();
        if b[4] != b':' || b[7] != b':' || b[10] != b' ' || b[13] != b':' || b[16] != b':' {
            return None;
        }
        if !b.iter().enumerate().all(|(i, &c)| match i {
            4 | 7 | 10 | 13 | 16 => true,
            _ => c.is_ascii_digit(),
        }) {
            return None;
        }
        let yil: u16 = m[0..4].parse().ok()?;
        let ay: u8 = m[5..7].parse().ok()?;
        let gun: u8 = m[8..10].parse().ok()?;
        let saat: u8 = m[11..13].parse().ok()?;
        let dakika: u8 = m[14..16].parse().ok()?;
        let saniye: u8 = m[17..19].parse().ok()?;
        if ay == 0 || ay > 12 || gun == 0 || gun > 31 || saat > 23 || dakika > 59 || saniye > 59 {
            return None;
        }
        Some(CekimZamani {
            yil,
            ay,
            gun,
            saat,
            dakika,
            saniye,
        })
    }

    /// ISO 8601 benzeri kısa biçim: `2026-09-29T13:45:07`.
    ///
    /// Sıralama anahtarı ve rapor etiketi olarak kullanılır; saat dilimi
    /// bilinmediği için **yerel saat** olarak yorumlanır.
    pub fn iso8601(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            self.yil, self.ay, self.gun, self.saat, self.dakika, self.saniye
        )
    }

    /// İki zaman arasındaki farkı saniye olarak verir.
    ///
    /// Fark negatifse `None` döner: "kare B, kareden A önce çekildi" durumu
    /// zaman damgası yerine dosya sırasını bozmak yerine reddedilir.
    pub fn fark_saniye(&self, diger: &CekimZamani) -> Option<i64> {
        let a = self.toplam_saniye();
        let b = diger.toplam_saniye();
        b.checked_sub(a)
    }

    /// Geçici bir epoch değerine çevirir (gün hesabı, artık yıl düzeltmeli).
    ///
    /// Bu değer **karşılaştırma** içindir; raporda mutlak tarih olarak
    /// gösterilmez, `iso8601()` kullanılır.
    pub fn toplam_saniye(&self) -> i64 {
        let yil = i64::from(self.yil);
        let ay = i64::from(self.ay);
        // 1970'ten bu yana geçen yıl sayısı (artık yıl kaba tahmini).
        let yil_farki = yil - 1970;
        let gunlar = yil_farki * 365 + yil_farki / 4 - yil_farki / 100 + yil_farki / 400;
        let gun_ek = i64::from(self.gun) - 1;
        let saat =
            i64::from(self.saat) * 3600 + i64::from(self.dakika) * 60 + i64::from(self.saniye);
        (gunlar * 366 + (ay - 1) * 31 + gun_ek) * 86_400 + saat
    }
}

/// Piksel yoğunluğu (piksel/mm²) — crop faktörü çıkarımı için.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PikselYogunlugu {
    /// Yatay çözünürlük (piksel).
    pub genislik: u32,
    /// Dikey çözünürlük (piksel).
    pub yukseklik: u32,
    /// Odak uzaklığı (mm).
    pub odak_mm: f32,
}

impl PikselYogunlugu {
    /// Yaklaşık crop faktörü (tam kare 35 mm referansına göre).
    ///
    /// 35 mm tam karede ~4,3 Mpx/mm² kabul edilir (film formatı ISO 126).
    /// Sonuç, sensör boyutu bilinmediği için bir **tahmindir**; raporun
    /// "Bilinen Sınırlamalar" bölümünde belgelenir.
    pub fn crop_faktor(&self) -> f64 {
        if self.odak_mm <= 0.0 || self.genislik == 0 || self.yukseklik == 0 {
            return 0.0;
        }
        let alan_mm2 = (f64::from(self.genislik) * f64::from(self.yukseklik))
            / (f64::from(self.odak_mm) * f64::from(self.odak_mm));
        if alan_mm2 <= 0.0 {
            return 0.0;
        }
        43.266 / alan_mm2.sqrt()
    }
}

/// Bir dosyanın çözülmüş fotoğraf metadatası.
#[derive(Debug, Clone, Default)]
pub struct Meta {
    /// Kamera üreticisi.
    pub make: Option<String>,
    /// Kamera modeli.
    pub model: Option<String>,
    /// Yazılım alanı (bazı üreticiler doldurur).
    pub yazilim: Option<String>,
    /// Çekim zamanı (`DateTimeOriginal`, yoksa `DateTime`).
    pub zaman: Option<CekimZamani>,
    /// Dikey poz süresi (saniye).
    pub poz_suresi: Option<f32>,
    /// Diyafram açıklığı.
    pub diyafram: Option<f32>,
    /// ISO duyarlılığı.
    pub iso: Option<u32>,
    /// Poz telafisi (EV).
    pub poz_bias: Option<f32>,
    /// Odak uzaklığı (mm).
    pub odak_mm: Option<f32>,
    /// Yön kodunun sayısal değeri (1-8).
    pub yon_kodu: Option<u16>,
    /// Piksel genişliği.
    pub genislik: Option<u32>,
    /// Piksel yüksekliği.
    pub yukseklik: Option<u32>,
    /// Sıkıştırma kodu (7 = eski tip JPEG, 6 = JPEG).
    pub sikistirma: Option<u16>,
    /// Örnek/piksel bileşen sayısı.
    pub ornek_adet: Option<u16>,
    /// Ölçüm modu.
    pub olcum_modu: Option<u16>,
    /// Pozlama programı.
    pub poz_programi: Option<u16>,
    /// Flaş alanı.
    pub flas: Option<u16>,
    /// Beyaz dengesi.
    pub beyaz_denge: Option<u16>,
    /// Fotometrik yorum kodu.
    pub fotometrik: Option<u16>,
    /// Toplam şerit baytı (`StripByteCounts`); ham veri okunmaz.
    pub serit_bayt: Option<u32>,
    /// Sıkıştırma kodu 7 ise ham veri JPEG'dir (TIFF-eski-tipi sıkıştırma).
    pub serit_jpeg_mi: bool,
    /// Okunan toplam bayt (kısmi okuma kanıtı).
    pub okunan_bayt: u64,
    /// Dosyanın toplam bayt uzunluğu.
    pub dosya_boyutu: u64,
}

impl Meta {
    /// Tam kare 35 mm'ye göre tahmini crop faktörü.
    pub fn crop_faktor(&self) -> f64 {
        let g = self.genislik.unwrap_or(0);
        let y = self.yukseklik.unwrap_or(0);
        match self.odak_mm {
            Some(f) if f > 0.0 && g > 0 && y > 0 => PikselYogunlugu {
                genislik: g,
                yukseklik: y,
                odak_mm: f,
            }
            .crop_faktor(),
            _ => 0.0,
        }
    }

    /// Yön kodundan fiziksel en-boy oranını düzeltir.
    ///
    /// 5-8 arası kodlar 90° döndürme bildirir; bu durumda genişlik ve yükseklik
    /// yer değiştirir. Kompozisyon puanı bu düzeltilmiş oranı kullanır.
    pub fn duzeltilmis_boyut(&self) -> Option<(u32, u32)> {
        let g = self.genislik?;
        let y = self.yukseklik?;
        match self.yon_kodu {
            Some(5) | Some(6) | Some(7) | Some(8) => Some((y, g)),
            _ => Some((g, y)),
        }
    }

    /// Yön uygulanmış en-boy oranı.
    pub fn en_boy_orani(&self) -> Option<f64> {
        let (g, y) = self.duzeltilmis_boyut()?;
        if y == 0 {
            return None;
        }
        Some(f64::from(g) / f64::from(y))
    }

    /// Efektif poz süresi — saniye cinsinden, `1/250` gibi bir değeri `0.004` yapar.
    pub fn hiz_saniye(&self) -> Option<f32> {
        self.poz_suresi
    }
}

/// Girdi listesinden ilk eşleşen etiketi döndürür (IFD0 ve Exif birlikte aranır).
fn ilk_girdi<'a>(belge: &'a TiffBelge, etiketler: &[u16]) -> Option<&'a Girdi> {
    etiketler.iter().find_map(|e| belge.etiket(*e))
}

/// Çözülmüş TIFF belgesinden anlamsal metadatası üretir.
///
/// Yalnızca ihtiyaç duyulan ~20 etiketin değeri okunur; ham piksel verisi
/// hiç okunmaz. Alan yoksa `None` yazılır — hataya dönüşmez.
pub fn coz_belge(okuyucu: &mut TiffOkuyucu, belge: &TiffBelge) -> Sonuc<Meta> {
    let mut meta = Meta {
        okunan_bayt: belge.okunan_bayt,
        dosya_boyutu: belge.dosya_boyutu,
        ..Meta::default()
    };

    macro_rules! metin {
        ($etiketler:expr) => {{
            match ilk_girdi(belge, $etiketler) {
                Some(g) => okuyucu.deger_metin(g)?,
                None => None,
            }
        }};
    }
    macro_rules! sayi {
        ($etiketler:expr) => {{
            match ilk_girdi(belge, $etiketler) {
                Some(g) => okuyucu.deger_u16(g)?,
                None => None,
            }
        }};
    }
    macro_rules! ondalik {
        ($etiketler:expr) => {{
            match ilk_girdi(belge, $etiketler) {
                Some(g) => okuyucu.deger_f32(g)?,
                None => None,
            }
        }};
    }

    meta.make = metin!(&[tiff::ETIKET_MAKE]);
    meta.model = metin!(&[tiff::ETIKET_MODEL]);
    meta.yazilim = metin!(&[ETIKET_YAZILIM]);

    if let Some(metin) = metin!(&[ETIKET_CEKIM_TARIHI, ETIKET_TARIH]) {
        meta.zaman = CekimZamani::ayrıştir(&metin);
    }

    meta.poz_suresi = ondalik!(&[ETIKET_POZ_SURESI]);
    meta.diyafram = ondalik!(&[ETIKET_DIYAFRAM]);
    meta.odak_mm = ondalik!(&[ETIKET_ODAK_UZAKLIGI]);
    meta.poz_bias = ondalik!(&[ETIKET_POZ_BIAS]);
    meta.iso = sayi!(&[ETIKET_ISO]).map(u32::from);
    meta.yon_kodu = sayi!(&[ETIKET_YON]);
    meta.sikistirma = sayi!(&[ETIKET_SIKISTIRMA]);
    meta.ornek_adet = sayi!(&[ETIKET_ORNEK_ADET]);
    meta.olcum_modu = sayi!(&[ETIKET_OLCUM_MODU]);
    meta.poz_programi = sayi!(&[ETIKET_POZ_PROGRAMI]);
    meta.flas = sayi!(&[ETIKET_FLAS]);
    meta.beyaz_denge = sayi!(&[ETIKET_BEYAZ_DENGES]);
    meta.fotometrik = sayi!(&[ETIKET_FOTOMETRIK]);

    // Genişlik/yükseklik: geçerli piksel boyutları (Exif) önceliklidir.
    meta.genislik = sayi!(&[ETIKET_PIXEL_X, ETIKET_GENISLIK]).map(u32::from);
    meta.yukseklik = sayi!(&[ETIKET_PIXEL_Y, ETIKET_UZUNLUK]).map(u32::from);

    // Şerit uzunluğu yalnızca JPEG sıkıştırmada anlamlıdır.
    if meta.sikistirma == Some(7) {
        if let Some(g) = belge.etiket(ETIKET_SERIT_UZUNLUK) {
            meta.serit_bayt = okuyucu.deger_u32(g)?;
            meta.serit_jpeg_mi = true;
        }
    }

    Ok(meta)
}

/// Bir dizinin girdilerinden etiket listesi çıkarır (test/diyagnostik amaçlı).
pub fn dizin_etiketleri(ifd: &Ifd) -> Vec<u16> {
    ifd.girdiler.iter().map(|g| g.etiket).collect()
}

/// Girdinin tipi `Undefined` ise işaretler (JFIF/ICC gibi ham blob'lar).
pub fn ham_blob_mu(tip: Option<AlanTipi>) -> bool {
    tip == Some(AlanTipi::Tanimsiz)
}

#[cfg(test)]
// Gerekçe: `unwrap`/`expect` yalnızca test içinde kullanılır.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn tarih_standart_bicimi_ayristirir() {
        let z = CekimZamani::ayrıştir("2026:09:29 13:45:07").expect("ayrıştır");
        assert_eq!(z.yil, 2026);
        assert_eq!(z.ay, 9);
        assert_eq!(z.gun, 29);
        assert_eq!(z.saat, 13);
        assert_eq!(z.dakika, 45);
        assert_eq!(z.saniye, 7);
    }

    #[test]
    fn tarih_iso8601_bicimine_cevrilir() {
        let z = CekimZamani::ayrıştir("2026:09:29 13:45:07").expect("ayrıştır");
        assert_eq!(z.iso8601(), "2026-09-29T13:45:07");
    }

    #[test]
    fn tarih_nul_sonrasini_keser() {
        let z = CekimZamani::ayrıştir("2026:09:29 13:45:07\0\0garbage").expect("ayrıştır");
        assert_eq!(z.gun, 29);
    }

    #[test]
    fn tarih_bozuk_girdileri_reddeder() {
        let kotu = [
            "",
            "2026-09-29 13:45:07",  // yanlış ayırıcı
            "2026:09:29 13:45",     // eksik alan
            "2026:09:29T13:45:07",  // T yerine boşluk bekleniyor
            "2026:13:29 13:45:07",  // ay 12'den büyük
            "2026:00:29 13:45:07",  // ay 0
            "2026:09:00 13:45:07",  // gün 0
            "2026:09:32 13:45:07",  // gün 31'den büyük
            "2026:09:29 24:45:07",  // saat 24
            "2026:09:29 13:60:07",  // dakika 60
            "2026:09:29 13:45:60",  // saniye 60
            "20260929 134507",      // ayırıcısız
            "2026:09:29 13:45:07x", // ek karakter
            "20a6:09:29 13:45:07",  // rakam olmayan
        ];
        for m in kotu {
            assert!(
                CekimZamani::ayrıştir(m).is_none(),
                "reddedilmesi gereken: {}",
                m
            );
        }
    }

    #[test]
    fn tarih_farki_saniye_uzerinden_hesaplanir() {
        let a = CekimZamani::ayrıştir("2026:09:29 13:45:00").expect("a");
        let b = CekimZamani::ayrıştir("2026:09:29 13:45:10").expect("b");
        assert_eq!(a.fark_saniye(&b), Some(10));
        assert_eq!(b.fark_saniye(&a), Some(-10));
    }

    #[test]
    fn tarih_farki_gunler_arasinda_pozitiftir() {
        let a = CekimZamani::ayrıştir("2026:09:29 23:59:59").expect("a");
        let b = CekimZamani::ayrıştir("2026:09:30 00:00:01").expect("b");
        let fark = a.fark_saniye(&b).expect("fark");
        assert!(fark > 0, "{}", fark);
        assert!(fark <= 3 * 86_400, "{}", fark);
    }

    #[test]
    fn tarih_sirası_ayrik_ay_gectiginde_ilerler() {
        let a = CekimZamani::ayrıştir("2026:01:31 00:00:00").expect("a");
        let b = CekimZamani::ayrıştir("2026:02:01 00:00:00").expect("b");
        assert!(a < b);
        assert!(b.toplam_saniye() > a.toplam_saniye());
    }

    #[test]
    fn crop_faktoru_alan_yogunluk_artinca_azalir() {
        let genis = PikselYogunlugu {
            genislik: 6000,
            yukseklik: 4000,
            odak_mm: 50.0,
        };
        let dar = PikselYogunlugu {
            genislik: 3000,
            yukseklik: 2000,
            odak_mm: 50.0,
        };
        let fg = genis.crop_faktor();
        let fd = dar.crop_faktor();
        assert!(fg > 0.0 && fd > 0.0, "{} {}", fg, fd);
        assert!(fg < fd, "yoğun sensör -> küçük crop: {} {}", fg, fd);
    }

    #[test]
    fn crop_faktoru_odak_uzakligi_artan_kamerada_buyur() {
        // Aynı çözünürlük, uzun odak = dar görüş açısı = büyük crop faktörü.
        let a = PikselYogunlugu {
            genislik: 4000,
            yukseklik: 3000,
            odak_mm: 24.0,
        };
        let b = PikselYogunlugu {
            genislik: 4000,
            yukseklik: 3000,
            odak_mm: 200.0,
        };
        assert!(
            b.crop_faktor() > a.crop_faktor(),
            "200mm {} 24mm {}",
            b.crop_faktor(),
            a.crop_faktor()
        );
    }

    #[test]
    fn crop_faktoru_sifir_odakta_sifirdir() {
        let p = PikselYogunlugu {
            genislik: 4000,
            yukseklik: 3000,
            odak_mm: 0.0,
        };
        assert_eq!(p.crop_faktor(), 0.0);
    }

    #[test]
    fn crop_faktoru_sifir_boyutta_sifirdir() {
        let p = PikselYogunlugu {
            genislik: 0,
            yukseklik: 3000,
            odak_mm: 50.0,
        };
        assert_eq!(p.crop_faktor(), 0.0);
    }

    fn meta_ile(boyut: (u32, u32), yon: Option<u16>) -> Meta {
        Meta {
            genislik: Some(boyut.0),
            yukseklik: Some(boyut.1),
            yon_kodu: yon,
            ..Meta::default()
        }
    }

    #[test]
    fn duzeltilmis_boyut_dikey_yon_degistirir() {
        let m = meta_ile((4000, 3000), Some(6));
        assert_eq!(m.duzeltilmis_boyut(), Some((3000, 4000)));
        let r = m.en_boy_orani().expect("oran");
        assert!((r - 0.75).abs() < 1e-9, "{}", r);
    }

    #[test]
    fn duzeltilmis_boyut_yatay_yon_degistirmez() {
        let m = meta_ile((4000, 3000), Some(1));
        assert_eq!(m.duzeltilmis_boyut(), Some((4000, 3000)));
        let r = m.en_boy_orani().expect("oran");
        assert!((r - 4.0 / 3.0).abs() < 1e-9, "{}", r);
    }

    #[test]
    fn yon_kodu_5_ile_8_arasi_degistirir() {
        for kod in [5u16, 6, 7, 8] {
            let m = meta_ile((4000, 3000), Some(kod));
            assert_eq!(m.duzeltilmis_boyut(), Some((3000, 4000)), "kod {}", kod);
        }
    }

    #[test]
    fn duzeltilmis_boyut_yon_bilinmiyorsa_dokunmaz() {
        let m = meta_ile((4000, 3000), None);
        assert_eq!(m.duzeltilmis_boyut(), Some((4000, 3000)));
    }

    #[test]
    fn en_boy_orani_sifir_yukseklikte_none_doner() {
        let m = meta_ile((4000, 0), Some(1));
        assert!(m.en_boy_orani().is_none());
    }

    #[test]
    fn boyut_yoksa_duzeltilmis_boyut_none() {
        let m = Meta::default();
        assert_eq!(m.duzeltilmis_boyut(), None);
        assert!(m.en_boy_orani().is_none());
    }

    #[test]
    fn meta_crop_faktoru_odak_yoksa_sifirdir() {
        let m = meta_ile((4000, 3000), Some(1));
        assert_eq!(m.crop_faktor(), 0.0);
    }

    #[test]
    fn hiz_saniye_poz_suresini_aynen_dondurur() {
        let m = Meta {
            poz_suresi: Some(0.004),
            ..Meta::default()
        };
        assert_eq!(m.hiz_saniye(), Some(0.004));
    }

    #[test]
    fn dizin_etiketleri_ayikti() {
        let ifd = Ifd {
            girdiler: vec![
                Girdi {
                    etiket: 1,
                    tip: Some(AlanTipi::Short),
                    adet: 1,
                    deger_ofseti: 0,
                    satir_ici: [0; 4],
                },
                Girdi {
                    etiket: 7,
                    tip: Some(AlanTipi::Long),
                    adet: 1,
                    deger_ofseti: 0,
                    satir_ici: [0; 4],
                },
            ],
        };
        assert_eq!(dizin_etiketleri(&ifd), vec![1, 7]);
    }

    #[test]
    fn ham_blob_tanimsiz_tipte_aranir() {
        assert!(ham_blob_mu(Some(AlanTipi::Tanimsiz)));
        assert!(!ham_blob_mu(Some(AlanTipi::Short)));
        assert!(!ham_blob_mu(None));
    }
}
