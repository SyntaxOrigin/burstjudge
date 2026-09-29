//! Gömülü JPEG önizleme çıkarımı — **çözmeden**, bayt aralığı olarak.
//!
//! # Kaynaklar (öncelik sırasıyla)
//!
//! 1. **`JPEGInterchangeFormat` (0x0201) + `JPEGInterchangeFormatLength` (0x0202).**
//!    CR2, NEF, ARW, DNG ve Canon/Nikon/Sony üreticilerinin fiili standardı.
//! 2. **`StripOffsets` (0x0111) + `StripByteCounts` (0x0117)**, yalnızca
//!    `Compression` = 7 (eski tip JPEG sıkıştırma) ise. Tek şeritli TIFF
//!    dosyalarında önizleme bu yolla bulunur.
//! 3. Hiçbiri yoksa [`OnizlemeYok`] hatası: dosya "önizlemesiz" olarak işaretlenir
//!    ve yalnızca metadatasıyla rapora girer (MANIFEST kartı 05, madde 2).
//!
//! # Çözme yok
//!
//! Bu modül JPEG'in **baytlarını kopyalar**, kod çözmez. Algısal hash üretmek
//! isteyen çağıran taraf, elde ettiği baytları [`crate::jpeg`] modülüne verir.
//! Çıkarım `Stream` üzerinden parça parça yazılır: önizleme 8 MB olsa bile
//! tampon 64 KiB'de kalır.

use std::io::{Read, Write};
use std::path::Path;

use crate::hata::{Hata, Sonuc};
use crate::tiff::{
    TiffBelge, TiffOkuyucu, ETIKET_JPEG_BASLANGIC, ETIKET_JPEG_UZUNLUK, ETIKET_SERIT_OFSET,
    ETIKET_SERIT_UZUNLUK, ETIKET_SIKISTIRMA,
};

/// Çıkarım sırasında tek seferde kopyalanan en fazla bayt.
///
/// 64 KiB. Önizleme boyutundan bağımsız sabit tampon.
pub const YAZMA_PARCA: usize = 64 * 1024;

/// Bir dosyanın gömülü JPEG önizlemesinin konumu ve uzunluğu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OnizlemeKonumu {
    /// Önizlemenin dosya içindeki başlangıç offset'i.
    pub ofset: u64,
    /// Önizlemenin bayt uzunluğu.
    pub uzunluk: u64,
    /// Konumun hangi etiketle bulunduğu (rapor ve tanı için kullanılır).
    pub kaynak: OnizlemeKaynagi,
}

/// Önizlemenin bulunduğu etiket çifti.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnizlemeKaynagi {
    /// `JPEGInterchangeFormat` (0x0201/0x0202).
    JpegInterchange,
    /// `StripOffsets` (0x0111) + `StripByteCounts` (0x0117), Compression = 7.
    Serit,
}

impl OnizlemeKaynagi {
    /// Raporda gösterilecek kısa etiket.
    pub fn etiket(&self) -> &'static str {
        match self {
            OnizlemeKaynagi::JpegInterchange => "JPEGInterchangeFormat",
            OnizlemeKaynagi::Serit => "StripOffsets",
        }
    }
}

impl std::fmt::Display for OnizlemeKaynagi {
    fn fmt(&self, bicik: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(bicik, "{}", self.etiket())
    }
}

/// Çözülmüş belge içinde gömülü önizleme arar.
///
/// Uzunluk sıfır veya offset kapsam dışıysa o aday yok sayılır ve bir sonraki
/// kaynak denenir. Dosya `TiffOkuyucu`'yu kademeli tükettirir; okuma sayacı
/// raporun "kısmi okuma" kanıtıdır.
pub fn onizleme_konumu(okuyucu: &mut TiffOkuyucu, belge: &TiffBelge) -> Sonuc<OnizlemeKonumu> {
    let dosya_boyutu = okuyucu.dosya_boyutu();

    // 1) JPEGInterchangeFormat — fiili standart.
    if let (Some(bas), Some(uzunluk_girdi)) = (
        belge.etiket(ETIKET_JPEG_BASLANGIC),
        belge.etiket(ETIKET_JPEG_UZUNLUK),
    ) {
        let bas = okuyucu.deger_u32(bas)?;
        let uzunluk = okuyucu.deger_u32(uzunluk_girdi)?;
        if let Some(k) = dogrula(bas, uzunluk, dosya_boyutu) {
            return Ok(k);
        }
    }

    // 2) StripOffsets + StripByteCounts, yalnız Compression = 7 (eski JPEG).
    let sikistirma_eski_jpeg = match belge.etiket(ETIKET_SIKISTIRMA) {
        Some(g) => okuyucu.deger_u16(g)? == Some(7),
        None => false,
    };
    if sikistirma_eski_jpeg {
        if let (Some(ofset_girdi), Some(uzunluk_girdi)) = (
            belge.etiket(ETIKET_SERIT_OFSET),
            belge.etiket(ETIKET_SERIT_UZUNLUK),
        ) {
            let ofset = okuyucu.deger_u32(ofset_girdi)?;
            let uzunluk = okuyucu.deger_u32(uzunluk_girdi)?;
            if let Some(mut k) = dogrula(ofset, uzunluk, dosya_boyutu) {
                k.kaynak = OnizlemeKaynagi::Serit;
                return Ok(k);
            }
        }
    }

    Err(Hata::OnizlemeYok {
        yol: okuyucu.yol().to_path_buf(),
    })
}

/// Offset + uzunluk çiftini dosya sınırına göre doğrular.
fn dogrula(ofset: Option<u32>, uzunluk: Option<u32>, dosya_boyutu: u64) -> Option<OnizlemeKonumu> {
    let ofset = u64::from(ofset?);
    let uzunluk = u64::from(uzunluk?);
    if uzunluk == 0 || ofset == 0 || ofset >= dosya_boyutu {
        return None;
    }
    if ofset.saturating_add(uzunluk) > dosya_boyutu {
        return None;
    }
    Some(OnizlemeKonumu {
        ofset,
        uzunluk,
        kaynak: OnizlemeKaynagi::JpegInterchange,
    })
}

/// Önizlemeyi `Stream` olarak okur: en fazla `ust_sinir` bayt döndürür.
///
/// Ham sensör verisi **okunmaz**; yalnız bulunan JPEG aralığı kopyalanır.
/// Önizleme `ust_sinir` baytı aşıyorsa hata döner (bellek bütçesi koruması).
pub fn onizleme_baytlarini_oku(
    okuyucu: &mut TiffOkuyucu,
    konum: &OnizlemeKonumu,
    ust_sinir: usize,
) -> Sonuc<Vec<u8>> {
    if konum.uzunluk == 0 {
        return Err(Hata::OnizlemeYok {
            yol: okuyucu.yol().to_path_buf(),
        });
    }
    if konum.uzunluk > ust_sinir as u64 {
        return Err(Hata::IfdGecersiz {
            yol: okuyucu.yol().to_path_buf(),
            ayrinti: format!(
                "önizleme {} bayt > {} bayt bellek sınırı",
                konum.uzunluk, ust_sinir
            ),
        });
    }
    let mut hedef = vec![0u8; konum.uzunluk as usize];
    okuyucu
        .aralik_oku(konum.ofset, &mut hedef)
        .map_err(|_| Hata::OnizlemeBozuk {
            yol: okuyucu.yol().to_path_buf(),
            ayrinti: format!("{} baytlık aralık okunamadı", konum.uzunluk),
        })?;
    Ok(hedef)
}

/// Önizlemeyi hedef `Write` akışına **parça parça** kopyalar.
///
/// Kaynak dosyanın tamamı belleğe alınmaz: `YAZMA_PARCA` baytlık tampon
/// kullanılır. Böylece 40 MB'lık bir önizleme 64 KiB tepe bellekle yazılır.
pub fn onizleme_yaz<W: Write>(
    okuyucu: &mut TiffOkuyucu,
    konum: &OnizlemeKonumu,
    hedef: &mut W,
) -> Sonuc<u64> {
    let mut yazilan: u64 = 0;
    let mut kalan = konum.uzunluk;
    let mut ofset = konum.ofset;
    let mut tampon = vec![0u8; YAZMA_PARCA];
    while kalan > 0 {
        let parca = kalan.min(YAZMA_PARCA as u64) as usize;
        okuyucu
            .aralik_oku(ofset, &mut tampon[..parca])
            .map_err(|_| Hata::OnizlemeBozuk {
                yol: okuyucu.yol().to_path_buf(),
                ayrinti: format!("{} bayt yazılamadı", kalan),
            })?;
        hedef
            .write_all(&tampon[..parca])
            .map_err(|kaynak| Hata::io("önizleme yazma", okuyucu.yol(), kaynak))?;
        yazilan += parca as u64;
        ofset += parca as u64;
        kalan -= parca as u64;
    }
    Ok(yazilan)
}

/// Bir `Read` kaynağından JPEG başlığı okuyup `SOI` imzasını doğrular.
///
/// Kod çözme yapmadan, ilk iki baytın `0xFF 0xD8` olması beklenir. Bu, önizleme
/// aralığının gerçekten JPEG olduğunun en ucuz doğrulamasıdır.
pub fn jpeg_soi_dogrula(baytlar: &[u8]) -> bool {
    baytlar.len() >= 2 && baytlar[0] == 0xFF && baytlar[1] == 0xD8
}

/// Bir `Read` kaynağından tam önizlemeyi okur (küçük önizlemeler için).
///
/// Büyük önizlemeler için [`onizleme_yaz`] kullanılmalıdır; bu yardımcı
/// yalnızca `uzunluk` bayta sığan önizlemeler içindir. `yol` yalnızca hata
/// mesajında kullanılır.
pub fn onizleme_oku_stream<R: Read>(yol: &Path, kaynak: &mut R, uzunluk: usize) -> Sonuc<Vec<u8>> {
    let mut veri = vec![0u8; uzunluk];
    let mut dolu = 0usize;
    while dolu < uzunluk {
        match kaynak.read(&mut veri[dolu..]) {
            Ok(0) => {
                return Err(Hata::OnizlemeBozuk {
                    yol: yol.to_path_buf(),
                    ayrinti: format!("{} bayt beklendi, {} okundu", uzunluk, dolu),
                })
            }
            Ok(n) => dolu += n,
            Err(kaynak_h) => {
                return Err(Hata::OnizlemeBozuk {
                    yol: yol.to_path_buf(),
                    ayrinti: kaynak_h.to_string(),
                })
            }
        }
    }
    Ok(veri)
}

#[cfg(test)]
// Gerekçe: `unwrap`/`expect` yalnızca test içinde kullanılır.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::tiff::{self, TiffOkuyucu};
    use std::io::Cursor;
    use std::path::PathBuf;

    struct Gecici {
        yol: PathBuf,
    }

    impl Gecici {
        fn yeni(etiket: &str, icerik: &[u8]) -> Gecici {
            let yol =
                std::env::temp_dir().join(format!("bj-oniz-{}-{}", etiket, std::process::id()));
            std::fs::write(&yol, icerik).expect("gecici yazma");
            Gecici { yol }
        }
    }

    impl Drop for Gecici {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.yol);
        }
    }

    fn girdi(etiket: u16, tip: u16, adet: u32, deger: [u8; 4]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&etiket.to_le_bytes());
        v.extend_from_slice(&tip.to_le_bytes());
        v.extend_from_slice(&adet.to_le_bytes());
        v.extend_from_slice(&deger);
        v
    }

    /// IFD0'a iki LONG girdi koyar (etiket -> değer), ardından `ek` verisini ekler.
    fn tiff_iki_girdi(a: (u16, u32), b: (u16, u32), ek: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"II\x2a\x00");
        v.extend_from_slice(&8u32.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&girdi(a.0, 4, 1, a.1.to_le_bytes()));
        v.extend_from_slice(&girdi(b.0, 4, 1, b.1.to_le_bytes()));
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(ek);
        v
    }

    #[test]
    fn soi_dogrulama_dogru_imzayi_kabul_eder() {
        assert!(jpeg_soi_dogrula(&[0xFF, 0xD8, 0xFF, 0xE0]));
    }

    #[test]
    fn soi_dogrulama_yanlis_imzayi_reddeder() {
        assert!(!jpeg_soi_dogrula(&[0xFF, 0xD9]));
        assert!(!jpeg_soi_dogrula(&[0x00]));
        assert!(!jpeg_soi_dogrula(&[]));
    }

    #[test]
    fn jpeg_interchange_format_onizlemeyi_bulur() {
        // Gövde: 8 (başlık) + 2 + 24 + 4 = 38 bayt; JPEG 40. baytta başlar.
        let onizleme: Vec<u8> = (0..32u8).collect();
        let ham = tiff_iki_girdi(
            (ETIKET_JPEG_BASLANGIC, 38),
            (ETIKET_JPEG_UZUNLUK, 32),
            &onizleme,
        );
        let g = Gecici::yeni("jif", &ham);
        let (mut okuyucu, baslik) = TiffOkuyucu::ac(&g.yol).expect("ac");
        let mut ziyaret = std::collections::BTreeSet::new();
        let belge = okuyucu
            .ifd_oku(u64::from(baslik.ifd0_ofseti), &mut ziyaret, 1)
            .expect("ifd");
        let belge = tiff::TiffBelge {
            baslik,
            ifd0: belge,
            exif: None,
            gps: None,
            okunan_bayt: okuyucu.okunan_bayt(),
            dosya_boyutu: okuyucu.dosya_boyutu(),
        };
        let konum = onizleme_konumu(&mut okuyucu, &belge).expect("konum");
        assert_eq!(konum.ofset, 38);
        assert_eq!(konum.uzunluk, 32);
        assert_eq!(konum.kaynak, OnizlemeKaynagi::JpegInterchange);
    }

    #[test]
    fn sifir_uzunluklu_onizleme_yok_sayilir() {
        let ham = tiff_iki_girdi((ETIKET_JPEG_BASLANGIC, 38), (ETIKET_JPEG_UZUNLUK, 0), &[]);
        let g = Gecici::yeni("sifir", &ham);
        let (mut okuyucu, baslik) = TiffOkuyucu::ac(&g.yol).expect("ac");
        let mut ziyaret = std::collections::BTreeSet::new();
        let ifd0 = okuyucu
            .ifd_oku(u64::from(baslik.ifd0_ofseti), &mut ziyaret, 1)
            .expect("ifd");
        let belge = tiff::TiffBelge {
            baslik,
            ifd0,
            exif: None,
            gps: None,
            okunan_bayt: 0,
            dosya_boyutu: okuyucu.dosya_boyutu(),
        };
        assert!(onizleme_konumu(&mut okuyucu, &belge).is_err());
    }

    #[test]
    fn offset_dosya_sini_disinda_onizleme_yok_sayilir() {
        let ham = tiff_iki_girdi(
            (ETIKET_JPEG_BASLANGIC, 900_000),
            (ETIKET_JPEG_UZUNLUK, 32),
            &[],
        );
        let g = Gecici::yeni("uzak", &ham);
        let (mut okuyucu, baslik) = TiffOkuyucu::ac(&g.yol).expect("ac");
        let mut ziyaret = std::collections::BTreeSet::new();
        let ifd0 = okuyucu
            .ifd_oku(u64::from(baslik.ifd0_ofseti), &mut ziyaret, 1)
            .expect("ifd");
        let belge = tiff::TiffBelge {
            baslik,
            ifd0,
            exif: None,
            gps: None,
            okunan_bayt: 0,
            dosya_boyutu: okuyucu.dosya_boyutu(),
        };
        assert!(onizleme_konumu(&mut okuyucu, &belge).is_err());
    }

    #[test]
    fn onizleme_baytlari_okunur() {
        let onizleme: Vec<u8> = (0..16u8).collect();
        let ham = tiff_iki_girdi(
            (ETIKET_JPEG_BASLANGIC, 38),
            (ETIKET_JPEG_UZUNLUK, 16),
            &onizleme,
        );
        let g = Gecici::yeni("oku", &ham);
        let (mut okuyucu, baslik) = TiffOkuyucu::ac(&g.yol).expect("ac");
        let mut ziyaret = std::collections::BTreeSet::new();
        let ifd0 = okuyucu
            .ifd_oku(u64::from(baslik.ifd0_ofseti), &mut ziyaret, 1)
            .expect("ifd");
        let belge = tiff::TiffBelge {
            baslik,
            ifd0,
            exif: None,
            gps: None,
            okunan_bayt: 0,
            dosya_boyutu: okuyucu.dosya_boyutu(),
        };
        let konum = onizleme_konumu(&mut okuyucu, &belge).expect("konum");
        let baytlar = onizleme_baytlarini_oku(&mut okuyucu, &konum, 1 << 20).expect("bayt");
        assert_eq!(baytlar, onizleme);
    }

    #[test]
    fn onizleme_bellek_siniri_ustu_reddedilir() {
        let onizleme: Vec<u8> = (0..16u8).collect();
        let ham = tiff_iki_girdi(
            (ETIKET_JPEG_BASLANGIC, 38),
            (ETIKET_JPEG_UZUNLUK, 16),
            &onizleme,
        );
        let g = Gecici::yeni("sinir", &ham);
        let (mut okuyucu, baslik) = TiffOkuyucu::ac(&g.yol).expect("ac");
        let mut ziyaret = std::collections::BTreeSet::new();
        let ifd0 = okuyucu
            .ifd_oku(u64::from(baslik.ifd0_ofseti), &mut ziyaret, 1)
            .expect("ifd");
        let belge = tiff::TiffBelge {
            baslik,
            ifd0,
            exif: None,
            gps: None,
            okunan_bayt: 0,
            dosya_boyutu: okuyucu.dosya_boyutu(),
        };
        let konum = onizleme_konumu(&mut okuyucu, &belge).expect("konum");
        assert!(onizleme_baytlarini_oku(&mut okuyucu, &konum, 4).is_err());
    }

    #[test]
    fn onizleme_yazma_parca_parca_calisir() {
        let onizleme: Vec<u8> = (0..16u8).collect();
        let ham = tiff_iki_girdi(
            (ETIKET_JPEG_BASLANGIC, 38),
            (ETIKET_JPEG_UZUNLUK, 16),
            &onizleme,
        );
        let g = Gecici::yeni("yaz", &ham);
        let (mut okuyucu, baslik) = TiffOkuyucu::ac(&g.yol).expect("ac");
        let mut ziyaret = std::collections::BTreeSet::new();
        let ifd0 = okuyucu
            .ifd_oku(u64::from(baslik.ifd0_ofseti), &mut ziyaret, 1)
            .expect("ifd");
        let belge = tiff::TiffBelge {
            baslik,
            ifd0,
            exif: None,
            gps: None,
            okunan_bayt: 0,
            dosya_boyutu: okuyucu.dosya_boyutu(),
        };
        let konum = onizleme_konumu(&mut okuyucu, &belge).expect("konum");
        let mut hedef: Vec<u8> = Vec::new();
        let yazilan = onizleme_yaz(&mut okuyucu, &konum, &mut hedef).expect("yaz");
        assert_eq!(yazilan, 16);
        assert_eq!(hedef, onizleme);
    }

    #[test]
    fn kaynak_etiketleri_dogru_yazilir() {
        assert_eq!(
            OnizlemeKaynagi::JpegInterchange.etiket(),
            "JPEGInterchangeFormat"
        );
        assert_eq!(OnizlemeKaynagi::Serit.etiket(), "StripOffsets");
        assert_eq!(OnizlemeKaynagi::Serit.to_string(), "StripOffsets");
    }

    #[test]
    fn stream_onizleme_oku_kisa_akista_hata_verir() {
        let veri: &[u8] = &[1, 2, 3];
        let mut kaynak = Cursor::new(veri);
        let yol = Path::new("akbozuk.jpg");
        assert!(onizleme_oku_stream(yol, &mut kaynak, 10).is_err());
    }

    #[test]
    fn stream_onizleme_oku_tam_akista_calisir() {
        let veri: &[u8] = &[1, 2, 3];
        let mut kaynak = Cursor::new(veri);
        let yol = Path::new("akyeni.jpg");
        let okunan = onizleme_oku_stream(yol, &mut kaynak, 3).expect("oku");
        assert_eq!(okunan, vec![1, 2, 3]);
    }
}
