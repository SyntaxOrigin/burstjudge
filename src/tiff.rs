//! TIFF/EXIF IFD ayrıştırıcısı — kademeli, dosyanın yalnız dizin bölümünü okur.
//!
//! # Ne yapar
//!
//! Klasik TIFF (sürüm 42) başlığını okur, IFD0 dizinini ve `ExifIFDPointer`
//! (0x8769) ile gösterilen Exif IFD dizinini çözümler. Her dizin girdisi 12
//! bayttır (etiket, tip, adet, değer/offset); tip başına bayt genişliği
//! tablodan gelir. Değer 4 bayttan büyükse dosyada başka bir yerde durur ve
//! **yalnızca ilgilenilen etiketler için** kademeli olarak okunur.
//!
//! # Ne yapmaz
//!
//! Piksel verisi, sıkıştırma, demosaic ve tam çözüm yoktur. `StripOffsets`
//! içerdiği ham veri bloğu hiç okunmaz; yalnızca `JPEGInterchangeFormat`
//! işaretlediği önizleme aralığı hedeflenir. Demosaic, renk uzayı dönüşümü
//! ve kırpma hiç uygulanmaz.
//!
//! # Güvenlik sınırları (rapor `b07` "bellek bütçesi" gereksinimi)
//!
//! - **BigTIFF reddi.** Sürüm 43 imzası (`II+` / `MM+`) açıkça hata döndürür;
//!   sessizce yanlış offset okunmaz.
//! - **Dizin adedi sınırı.** `IFD_GIRD_LIMITI` girdiden fazlasını reddeder.
//! - **Offset kapsam denetimi.** Her offset `dosya_boyutu` içinde olmalıdır.
//! - **Döngü koruması.** Ziyaret edilen IFD offset'leri bir kümede tutulur;
//!   aynı offset ikinci kez görülürse hata verilir. Ayrıca toplam adım sayısı
//!   `IFD_ADIM_LIMITI` ile sınırlıdır. Böylece kendi kendini gösteren bir
//!   offset zinciri sonsuz döngü oluşturmaz.
//! - **Değer uzunluğu sınırı.** Tek bir etiketin değeri `DEGER_LIMITI` bayttan
//!   uzunsa okunmaz; bu, tek etiketle devasa bellek tahsisini engeller.
//!
//! Kaynaklar: TIFF 6.0 (Section 2) ve Exif 2.3 (Section 4).

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::hata::{Hata, Sonuc};

/// Tek bir IFD içinde kabul edilen en fazla girdi sayısı.
///
/// 256 girdi = 3 KiB dizin. Dizin taraması bunu geçen dosyayı bozuk sayar.
pub const IFD_GIRD_LIMITI: usize = 256;

/// Bir dosyada çözülecek en fazla IFD adımı (IFD0 + Exif + GPS + Interop).
///
/// `IFD_ADIM_LIMITI` aşıldığında döngü koruması devreye girer.
pub const IFD_ADIM_LIMITI: usize = 32;

/// Tek bir etiket değerinin okunabileceği en fazla bayt sayısı.
///
/// 8 KiB. Make/Model gibi metinler için fazlasıyla yeterli; devasa
/// `Undefined` blob'ları bu sınırda kesilir.
pub const DEGER_LIMITI: usize = 8 * 1024;

/// Dizin girdisi okunurken tek `read` çağrısında istenen en fazla bayt.
///
/// 4 KiB. Küçük tampon, işletim sistemi çağrısı sayısını sınırlar.
pub const PARCA_BOYUTU: usize = 4 * 1024;

/// TIFF bayt sırası.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaytSirasi {
    /// `II` — az öncelikli (küçük endian), ör. Intel/ARM.
    Kucuk,
    /// `MM` — büyük öncelikli (büyük endian), ör. eski makine ve Adobe.
    Buyuk,
}

impl BaytSirasi {
    /// `u16` bayt çiftini bu sıraya göre çözer.
    pub fn u16(&self, baytlar: [u8; 2]) -> u16 {
        match self {
            BaytSirasi::Kucuk => u16::from_le_bytes(baytlar),
            BaytSirasi::Buyuk => u16::from_be_bytes(baytlar),
        }
    }

    /// `u32` dörtlüsünü bu sıraya göre çözer.
    pub fn u32(&self, baytlar: [u8; 4]) -> u32 {
        match self {
            BaytSirasi::Kucuk => u32::from_le_bytes(baytlar),
            BaytSirasi::Buyuk => u32::from_be_bytes(baytlar),
        }
    }
}

/// TIFF alan tipi (TIFF 6.0 Section 18).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlanTipi {
    /// 1 — 8 bit işaretsiz.
    Bayt,
    /// 2 — NUL sonlu metin.
    Ascii,
    /// 3 — 16 bit işaretsiz.
    Short,
    /// 4 — 32 bit işaretsiz.
    Long,
    /// 5 — iki adet 32 bit: pay/pafta.
    Rasyonel,
    /// 6 — 8 bit işaretli.
    SBayt,
    /// 7 — 8 bit, tip belirsiz (ICC profilleri, GPS verisi).
    Tanimsiz,
    /// 8 — 16 bit işaretli.
    SShort,
    /// 9 — 32 bit işaretli.
    SLong,
    /// 10 — iki adet 32 bit işaretli.
    SRasyonel,
    /// 11 — 32 bit kayan nokta.
    Kayan32,
    /// 12 — 64 bit kayan nokta.
    Kayan64,
    /// 13 — 32 bit IFD offset'i.
    Ifd,
}

impl AlanTipi {
    /// TIFF 6.0'te tanımlı sayısal kodu çözer; bilinmeyen kod `None` verir.
    pub fn koddan(kod: u16) -> Option<AlanTipi> {
        Some(match kod {
            1 => AlanTipi::Bayt,
            2 => AlanTipi::Ascii,
            3 => AlanTipi::Short,
            4 => AlanTipi::Long,
            5 => AlanTipi::Rasyonel,
            6 => AlanTipi::SBayt,
            7 => AlanTipi::Tanimsiz,
            8 => AlanTipi::SShort,
            9 => AlanTipi::SLong,
            10 => AlanTipi::SRasyonel,
            11 => AlanTipi::Kayan32,
            12 => AlanTipi::Kayan64,
            13 => AlanTipi::Ifd,
            _ => return None,
        })
    }

    /// Tipin bayt cinsinden genişliği.
    pub fn genislik(&self) -> u64 {
        match self {
            AlanTipi::Bayt | AlanTipi::Ascii | AlanTipi::SBayt | AlanTipi::Tanimsiz => 1,
            AlanTipi::Short | AlanTipi::SShort => 2,
            AlanTipi::Long | AlanTipi::SLong | AlanTipi::Kayan32 | AlanTipi::Ifd => 4,
            AlanTipi::Rasyonel | AlanTipi::SRasyonel | AlanTipi::Kayan64 => 8,
        }
    }
}

/// Bir IFD dizin girdisi.
#[derive(Debug, Clone)]
pub struct Girdi {
    /// EXIF/TIFF etiket numarası (ör. 0x0110 = ImageWidth).
    pub etiket: u16,
    /// Alan tipi; bilinmeyen tipler `None` olarak saklanmaz, çözümleme durdurulmaz.
    pub tip: Option<AlanTipi>,
    /// Değerin eleman adedi.
    pub adet: u32,
    /// 4 bayttan büyük değerlerin dosya içindeki başlangıç offset'i.
    ///
    /// Değer satır içindeyse bu alan anlamsızdır; `deger()` doğru kaynağı seçer.
    pub deger_ofseti: u64,
    /// Girdinin ham 4 baytlık değer alanı.
    pub satir_ici: [u8; 4],
}

impl Girdi {
    /// Değerin toplam bayt uzunluğu (taşma korumalı).
    pub fn toplam_bayt(&self) -> u64 {
        match self.tip {
            Some(tip) => (tip.genislik()).saturating_mul(u64::from(self.adet)),
            None => 0,
        }
    }

    /// Değerin satır içinde mi yoksa dosyada başka bir yerde mi olduğunu söyler.
    pub fn satir_ici_mi(&self) -> bool {
        self.toplam_bayt() <= 4
    }
}

/// Çözülmüş tek bir IFD dizini.
#[derive(Debug, Clone, Default)]
pub struct Ifd {
    /// Girdiler dosyadaki sırayıyla saklanır (TIFF dizinleri etikete göre
    /// sıralı olmak zorunda değildir; üreticiler sıralamayı bozar).
    pub girdiler: Vec<Girdi>,
}

impl Ifd {
    /// Etikete göre ilk girdiyi arar.
    pub fn bul(&self, etiket: u16) -> Option<&Girdi> {
        self.girdiler.iter().find(|g| g.etiket == etiket)
    }

    /// Etiket var mı diye bakar.
    pub fn var(&self, etiket: u16) -> bool {
        self.bul(etiket).is_some()
    }
}

/// `ExifIFDPointer` (0x8769) etiketi.
pub const ETIKET_EXIF_IFD: u16 = 0x8769;
/// `GPSInfoIFDPointer` (0x8825) etiketi.
pub const ETIKET_GPS_IFD: u16 = 0x8825;
/// `InteroperabilityIFDPointer` (0xA005) etiketi.
pub const ETIKET_INTEROP_IFD: u16 = 0xA005;
/// `ImageWidth` (0x0100) etiketi.
pub const ETIKET_GENISLIK: u16 = 0x0100;
/// `ImageLength` (0x0101) etiketi.
pub const ETIKET_UZUNLUK: u16 = 0x0101;
/// `BitsPerSample` (0x0102) etiketi.
pub const ETIKET_BIT_ORNEK: u16 = 0x0102;
/// `Compression` (0x0103) etiketi.
pub const ETIKET_SIKISTIRMA: u16 = 0x0103;
/// `PhotometricInterpretation` (0x0106) etiketi.
pub const ETIKET_FOTOMETRIK: u16 = 0x0106;
/// `StripOffsets` (0x0111) etiketi.
pub const ETIKET_SERIT_OFSET: u16 = 0x0111;
/// `Orientation` (0x0112) etiketi.
pub const ETIKET_YON: u16 = 0x0112;
/// `SamplesPerPixel` (0x0115) etiketi.
pub const ETIKET_ORNEK_ADET: u16 = 0x0115;
/// `RowsPerStrip` (0x0116) etiketi.
pub const ETIKET_SERIT_YUKSEK: u16 = 0x0116;
/// `StripByteCounts` (0x0117) etiketi.
pub const ETIKET_SERIT_UZUNLUK: u16 = 0x0117;
/// `Make` (0x010F) etiketi.
pub const ETIKET_MAKE: u16 = 0x010F;
/// `Model` (0x0110) etiketi.
pub const ETIKET_MODEL: u16 = 0x0110;
/// `ImageDescription` (0x010E) etiketi.
pub const ETIKET_ACIKLAMA: u16 = 0x010E;
/// `Software` (0x0131) etiketi.
pub const ETIKET_YAZILIM: u16 = 0x0131;
/// `DateTime` (0x0132) etiketi — IFD0 çekim tarihi.
pub const ETIKET_TARIH: u16 = 0x0132;
/// `JPEGInterchangeFormat` (0x0201) — gömülü önizlemenin başlangıcı.
pub const ETIKET_JPEG_BASLANGIC: u16 = 0x0201;
/// `JPEGInterchangeFormatLength` (0x0202) — gömülü önizlemenin uzunluğu.
pub const ETIKET_JPEG_UZUNLUK: u16 = 0x0202;
/// `ExposureTime` (0x829A) — dikey poz süresi, saniye.
pub const ETIKET_POZ_SURESI: u16 = 0x829A;
/// `FNumber` (0x829D) — diyafram açıklığı.
pub const ETIKET_DIYAFRAM: u16 = 0x829D;
/// `ExposureBiasValue` (0x9204).
pub const ETIKET_POZ_BIAS: u16 = 0x9204;
/// `ISOSpeedRatings` (0x8827) — duyarlılık.
pub const ETIKET_ISO: u16 = 0x8827;
/// `DateTimeOriginal` (0x9003) — Exif çekim tarihi.
pub const ETIKET_CEKIM_TARIHI: u16 = 0x9003;
/// `ShutterSpeedValue` (0x9201) — APEX dikey hız.
pub const ETIKET_PERDE_HIZ: u16 = 0x9201;
/// `FocalLength` (0x920A) — mm.
pub const ETIKET_ODAK_UZAKLIGI: u16 = 0x920A;
/// `ExposureProgram` (0x8822).
pub const ETIKET_POZ_PROGRAMI: u16 = 0x8822;
/// `MeteringMode` (0x9207).
pub const ETIKET_OLCUM_MODU: u16 = 0x9207;
/// `Flash` (0x9209).
pub const ETIKET_FLAS: u16 = 0x9209;
/// `WhiteBalance` (0xA403).
pub const ETIKET_BEYAZ_DENGES: u16 = 0xA403;
/// `PixelXDimension` (0xA002) — geçerli piksel genişliği.
pub const ETIKET_PIXEL_X: u16 = 0xA002;
/// `PixelYDimension` (0xA003) — geçerli piksel yüksekliği.
pub const ETIKET_PIXEL_Y: u16 = 0xA003;

/// TIFF/EXIF başlığı, dizin çözülmeden önce okunur.
#[derive(Debug, Clone)]
pub struct Baslik {
    /// Bayt sırası.
    pub sira: BaytSirasi,
    /// IFD0 dizininin dosya içindeki offset'i.
    pub ifd0_ofseti: u32,
    /// Başlığın ilk önizleme offset'ini (bazı üreticiler yazar).
    pub ifd1_ofseti: u32,
}

/// Kademeli TIFF okuyucu: dosyayı `seek` + sınırlı `read` ile gezer.
#[derive(Debug)]
pub struct TiffOkuyucu {
    dosya: File,
    yol: PathBuf,
    boyut: u64,
    sira: BaytSirasi,
    ifd0_ofseti: u32,
    okunan: u64,
    tampon: Vec<u8>,
}

impl TiffOkuyucu {
    /// Dosyayı açar ve yalnız 8 baytlık başlığı okur.
    ///
    /// Geri kalanı okunmaz: IFD dizini ancak `ifd_oku` çağrıldığında, o da
    /// gerektiği kadar okunur.
    pub fn ac(yol: &Path) -> Sonuc<(Self, Baslik)> {
        let mut dosya =
            File::open(yol).map_err(|kaynak| Hata::io("TIFF dosyası açma", yol, kaynak))?;
        let boyut = dosya
            .metadata()
            .map_err(|kaynak| Hata::io("TIFF boyutu öğrenme", yol, kaynak))?
            .len();

        let mut ilk = [0u8; 8];
        let mut dolu = 0usize;
        while dolu < ilk.len() {
            let n = dosya
                .read(&mut ilk[dolu..])
                .map_err(|kaynak| Hata::io("TIFF başlığı okuma", yol, kaynak))?;
            if n == 0 {
                break;
            }
            dolu += n;
        }
        if dolu < 8 {
            return Err(Hata::TiffBazliDegil {
                yol: yol.to_path_buf(),
                ayrinti: format!("dosya yalnız {} bayt; TIFF başlığı 8 bayt ister", dolu),
            });
        }

        let (sira, sihir) = match (ilk[0], ilk[1]) {
            (b'I', b'I') => (BaytSirasi::Kucuk, 42u16),
            (b'M', b'M') => (BaytSirasi::Buyuk, 42u16),
            _ => {
                return Err(Hata::TiffBazliDegil {
                    yol: yol.to_path_buf(),
                    ayrinti: format!("bayt sırası imzası geçersiz: {:02X} {:02X}", ilk[0], ilk[1]),
                })
            }
        };
        if sihir != sira.u16([ilk[2], ilk[3]]) {
            // Sihir 43 = BigTIFF. Reddet, tahmin etme.
            return Err(Hata::BigTiffReddedildi {
                yol: yol.to_path_buf(),
            });
        }

        let ifd0_ofseti = sira.u32([ilk[4], ilk[5], ilk[6], ilk[7]]);
        // 10. bayttaki "ilk IFD offset'i" alanı yalnız II/MM düzeninde anlamlıdır
        // ve 8 baytlık ön okumada yok; PRINTER ve TIFF/EP dosyalarında
        // bulunmayabilir. Bu MVP'de okunmaz (bkz. Bilinen Sınırlamalar).
        let okuyucu = TiffOkuyucu {
            dosya,
            yol: yol.to_path_buf(),
            boyut,
            sira,
            ifd0_ofseti,
            okunan: 8,
            tampon: vec![0u8; PARCA_BOYUTU],
        };
        Ok((
            okuyucu,
            Baslik {
                sira,
                ifd0_ofseti,
                ifd1_ofseti: 0,
            },
        ))
    }

    /// Dosyanın toplam bayt uzunluğu.
    pub fn dosya_boyutu(&self) -> u64 {
        self.boyut
    }

    /// Şu ana kadar okunan bayt sayısı — "kısmi okuma" kanıtı olarak raporda kullanılır.
    pub fn okunan_bayt(&self) -> u64 {
        self.okunan
    }

    /// İncelenen dosyanın yolu.
    pub fn yol(&self) -> &Path {
        &self.yol
    }

    /// Bayt sırası.
    pub fn sira(&self) -> BaytSirasi {
        self.sira
    }

    /// Başlık bilgisi (dosya açılırken okundu).
    pub fn baslik(&self) -> Baslik {
        Baslik {
            sira: self.sira,
            ifd0_ofseti: self.ifd0_ofseti,
            ifd1_ofseti: 0,
        }
    }

    /// Offset'in dosya sınırları içinde olduğunu doğrular.
    fn kapsam_dogrula(&self, offset: u64, uzunluk: u64) -> Sonuc<()> {
        let son = offset.saturating_add(uzunluk);
        if offset >= self.boyut || son > self.boyut {
            return Err(Hata::IfdGecersiz {
                yol: self.yol.clone(),
                ayrinti: format!(
                    "offset {} + {} bayt, dosya sınırı {} dışında",
                    offset, uzunluk, self.boyut
                ),
            });
        }
        Ok(())
    }

    /// `offset` konumundan `hedef.len()` bayt okur ve doğrulayan genel okuma.
    ///
    /// Önizleme çıkarımı da bu yolu kullanır; kapsam denetimi burada yapılır.
    pub fn aralik_oku(&mut self, offset: u64, hedef: &mut [u8]) -> Sonuc<()> {
        self.kapsam_dogrula(offset, hedef.len() as u64)?;
        self.dosya
            .seek(SeekFrom::Start(offset))
            .map_err(|kaynak| Hata::io("TIFF konumlanma", &self.yol, kaynak))?;
        let mut dolu = 0usize;
        while dolu < hedef.len() {
            match self.dosya.read(&mut hedef[dolu..]) {
                Ok(0) => {
                    return Err(Hata::IfdGecersiz {
                        yol: self.yol.clone(),
                        ayrinti: format!("offset {} sonunda beklenenden az bayt", offset),
                    })
                }
                Ok(n) => dolu += n,
                Err(kaynak) => return Err(Hata::io("TIFF okuma", &self.yol, kaynak)),
            }
        }
        self.okunan += hedef.len() as u64;
        Ok(())
    }

    /// `offset` konumundaki `u16` değerini okur.
    pub fn u16_oku(&mut self, offset: u64) -> Sonuc<u16> {
        let mut tampon = [0u8; 2];
        self.aralik_oku(offset, &mut tampon)?;
        Ok(self.sira.u16(tampon))
    }

    /// `offset` konumundaki `u32` değerini okur.
    pub fn u32_oku(&mut self, offset: u64) -> Sonuc<u32> {
        let mut tampon = [0u8; 4];
        self.aralik_oku(offset, &mut tampon)?;
        Ok(self.sira.u32(tampon))
    }

    /// Bir IFD dizinini çözer.
    ///
    /// `ziyaret` kümesi döngü korumasıdır: aynı offset ikinci kez gelirse
    /// `IfdGecersiz` döner. `adim` sayacı zincir uzunluğunu sınırlar.
    pub fn ifd_oku(&mut self, offset: u64, ziyaret: &mut BTreeSet<u64>, adim: usize) -> Sonuc<Ifd> {
        if adim > IFD_ADIM_LIMITI {
            return Err(Hata::IfdSinirAsim {
                yol: self.yol.clone(),
                adim,
            });
        }
        if !ziyaret.insert(offset) {
            return Err(Hata::IfdGecersiz {
                yol: self.yol.clone(),
                ayrinti: format!("IFD offset döngüsü: {} ikinci kez görüldü", offset),
            });
        }
        self.kapsam_dogrula(offset, 2)?;

        let adet = self.u16_oku(offset)? as usize;
        if adet > IFD_GIRD_LIMITI {
            return Err(Hata::IfdGecersiz {
                yol: self.yol.clone(),
                ayrinti: format!("{} girdi > {} sınırı", adet, IFD_GIRD_LIMITI),
            });
        }
        if adet == 0 {
            return Ok(Ifd::default());
        }

        // Girdiler 12 bayt; hepsini tek seferde okumak yerine parça parça
        // okunur, böylece devasa dizinlerde de tampon sınırlı kalır.
        // Dizin girdileri sayım alanından (2 bayt) sonra başlar.
        let govde = adet * 12;
        self.kapsam_dogrula(offset + 2, (govde + 4) as u64)?;
        let mut ham = Vec::with_capacity(govde);
        let mut kalan = govde;
        while kalan > 0 {
            let parca = kalan.min(self.tampon.len());
            let bas = ham.len();
            ham.resize(bas + parca, 0);
            self.aralik_oku(offset + 2 + bas as u64, &mut ham[bas..bas + parca])?;
            kalan -= parca;
        }

        let sira = self.sira;
        let mut girdiler = Vec::with_capacity(adet);
        for i in 0..adet {
            let t = i * 12;
            let etiket = sira.u16([ham[t], ham[t + 1]]);
            let tip_kod = sira.u16([ham[t + 2], ham[t + 3]]);
            let adet_sayi = sira.u32([ham[t + 4], ham[t + 5], ham[t + 6], ham[t + 7]]);
            let satir_ici = [ham[t + 8], ham[t + 9], ham[t + 10], ham[t + 11]];
            let deger_ofseti = match sira {
                BaytSirasi::Kucuk => u32::from_le_bytes(satir_ici) as u64,
                BaytSirasi::Buyuk => u32::from_be_bytes(satir_ici) as u64,
            };
            girdiler.push(Girdi {
                etiket,
                // Bilinmeyen tip hata değildir: girdi atlanır ama dizin çözülür
                // (MANIFEST kartı 05 risk notu: "bilinmeyen etiketler hata değil").
                tip: AlanTipi::koddan(tip_kod),
                adet: adet_sayi,
                deger_ofseti,
                satir_ici,
            });
        }
        Ok(Ifd { girdiler })
    }

    /// Bir girdinin ham değer baytlarını okur.
    ///
    /// Değer 4 bayttan küçükse satır içinden alınır; değilse dosyadan
    /// kademeli okunur. `DEGER_LIMITI` aşılırsa hata verilir.
    pub fn deger(&mut self, girdi: &Girdi) -> Sonuc<Vec<u8>> {
        if girdi.tip.is_none() {
            return Err(Hata::IfdGecersiz {
                yol: self.yol.clone(),
                ayrinti: format!("etiket 0x{:04X} bilinmeyen tip", girdi.etiket),
            });
        }
        let toplam = girdi.toplam_bayt();
        if toplam == 0 {
            return Ok(Vec::new());
        }
        if toplam > DEGER_LIMITI as u64 {
            return Err(Hata::IfdGecersiz {
                yol: self.yol.clone(),
                ayrinti: format!(
                    "etiket 0x{:04X} değeri {} bayt > {} sınırı",
                    girdi.etiket, toplam, DEGER_LIMITI
                ),
            });
        }
        if girdi.satir_ici_mi() {
            return Ok(girdi.satir_ici[..toplam as usize].to_vec());
        }
        let mut hedef = vec![0u8; toplam as usize];
        self.aralik_oku(girdi.deger_ofseti, &mut hedef)?;
        Ok(hedef)
    }

    /// Girdinin ilk `u16` bileşenini döndürür (SHORT/LONG sıraya göre çözülür).
    pub fn deger_u16(&mut self, girdi: &Girdi) -> Sonuc<Option<u16>> {
        let tip = girdi.tip;
        Ok(match tip {
            Some(AlanTipi::Short) | Some(AlanTipi::SShort) => {
                let ham = self.deger(girdi)?;
                if ham.len() < 2 {
                    return Ok(None);
                }
                let v = self.sira.u16([ham[0], ham[1]]);
                Some(if tip == Some(AlanTipi::SShort) {
                    v as i16 as u16
                } else {
                    v
                })
            }
            Some(AlanTipi::Long) | Some(AlanTipi::SLong) | Some(AlanTipi::Ifd) => {
                let ham = self.deger(girdi)?;
                if ham.len() < 4 {
                    return Ok(None);
                }
                let v = self.sira.u32([ham[0], ham[1], ham[2], ham[3]]);
                Some(if v > u32::from(u16::MAX) {
                    u16::MAX
                } else {
                    v as u16
                })
            }
            _ => None,
        })
    }

    /// Girdinin ilk `u32` bileşenini döndürür (uzun offsetler için).
    pub fn deger_u32(&mut self, girdi: &Girdi) -> Sonuc<Option<u32>> {
        match girdi.tip {
            Some(AlanTipi::Long) | Some(AlanTipi::Ifd) | Some(AlanTipi::SLong) => {
                let ham = self.deger(girdi)?;
                if ham.len() < 4 {
                    return Ok(None);
                }
                Ok(Some(self.sira.u32([ham[0], ham[1], ham[2], ham[3]])))
            }
            Some(AlanTipi::Short) | Some(AlanTipi::SShort) => {
                Ok(self.deger_u16(girdi)?.map(u32::from))
            }
            _ => Ok(None),
        }
    }

    /// Girdiyi `f32` sayısal değere çevirir (RATIONAL dahil).
    ///
    /// Payda sıfır veya değer okunamazsa `None` döner; çağıran taraf
    /// "bu alan yok" ile "bu alan bozuk" ayrımını kendi kararı verir.
    pub fn deger_f32(&mut self, girdi: &Girdi) -> Sonuc<Option<f32>> {
        let Some(tip) = girdi.tip else {
            return Ok(None);
        };
        match tip {
            AlanTipi::Rasyonel | AlanTipi::SRasyonel => {
                let ham = self.deger(girdi)?;
                if ham.len() < 8 {
                    return Ok(None);
                }
                let sira = self.sira;
                let pay = sira.u32([ham[0], ham[1], ham[2], ham[3]]);
                let payda = sira.u32([ham[4], ham[5], ham[6], ham[7]]);
                if payda == 0 {
                    return Ok(None);
                }
                let pay = if tip == AlanTipi::SRasyonel {
                    pay as i32 as f64
                } else {
                    pay as f64
                };
                Ok(Some((pay / payda as f64) as f32))
            }
            AlanTipi::Short | AlanTipi::SShort => Ok(self.deger_u16(girdi)?.map(|v| {
                if tip == AlanTipi::SShort {
                    v as i16 as f32
                } else {
                    v as f32
                }
            })),
            AlanTipi::Long | AlanTipi::Ifd | AlanTipi::SLong => {
                Ok(self.deger_u32(girdi)?.map(|v| v as f32))
            }
            _ => Ok(None),
        }
    }

    /// ASCII/Undefined değeri temizlenmiş metne çevirir.
    ///
    /// Sondaki NUL ve sonrası atılır, baştaki/sondaki boşluklar kırpılır,
    /// geçersiz UTF-8 baytları `U+FFFD` olur. Böylece bozuk bir `Make`
    /// alanı taramayı durdurmaz.
    pub fn deger_metin(&mut self, girdi: &Girdi) -> Sonuc<Option<String>> {
        if girdi.tip != Some(AlanTipi::Ascii) && girdi.tip != Some(AlanTipi::Tanimsiz) {
            return Ok(None);
        }
        let ham = self.deger(girdi)?;
        let kes = ham.iter().position(|&b| b == 0).unwrap_or(ham.len());
        let metin = String::from_utf8_lossy(&ham[..kes]).trim().to_string();
        Ok(if metin.is_empty() { None } else { Some(metin) })
    }
}

/// Çözülmüş TIFF belgesi: IFD0, Exif IFD ve okuma istatistiği.
#[derive(Debug, Clone)]
pub struct TiffBelge {
    /// Başlık bilgisi.
    pub baslik: Baslik,
    /// IFD0 dizini.
    pub ifd0: Ifd,
    /// Exif IFD dizini (işaretçi yoksa `None`).
    pub exif: Option<Ifd>,
    /// GPS IFD dizini (işaretçi yoksa `None`).
    pub gps: Option<Ifd>,
    /// Okunan toplam bayt (dizin + okunan değerler).
    pub okunan_bayt: u64,
    /// Dosyanın toplam bayt uzunluğu.
    pub dosya_boyutu: u64,
}

impl TiffBelge {
    /// `exif` varsa orada, yoksa IFD0'da etiketi arar.
    ///
    /// Bazı üreticiler `DateTimeOriginal`'ı IFD0'a yazar; ikisini de kabul etmek
    /// alan kaybını önler.
    pub fn etiket(&self, etiket: u16) -> Option<&Girdi> {
        if let Some(exif) = &self.exif {
            if let Some(g) = exif.bul(etiket) {
                return Some(g);
            }
        }
        self.ifd0.bul(etiket)
    }

    /// Okunan baytın dosya boyutuna oranı — kısmi okumanın sayısal kanıtı.
    pub fn okuma_orani(&self) -> f64 {
        if self.dosya_boyutu == 0 {
            return 0.0;
        }
        self.okunan_bayt as f64 / self.dosya_boyutu as f64
    }
}

/// Dosyayı açar, IFD0 ve Exif IFD dizinlerini çözer.
///
/// GPS dizini yalnız işaretçi doğrulanırsa çözülür; konum verisi rapora taşınmaz.
pub fn coz(yol: &Path) -> Sonuc<TiffBelge> {
    let (mut okuyucu, _baslik) = TiffOkuyucu::ac(yol)?;
    coz_oku(&mut okuyucu)
}

/// Zaten açılmış bir okuyucu üzerinde IFD0 + Exif IFD çözümü yapar.
///
/// Tarama motoru dosyayı **bir kez** açar ve buradan sonra okuyucuyu kademeli
/// tüketir; bu yüzden `coz` yerine bu sürüm kullanılır.
pub fn coz_oku(okuyucu: &mut TiffOkuyucu) -> Sonuc<TiffBelge> {
    let dosya_boyutu = okuyucu.dosya_boyutu();
    let yol = okuyucu.yol().to_path_buf();
    let ifd0_ofseti = okuyucu.baslik().ifd0_ofseti;
    let ifd0_sira = okuyucu.sira();
    let _ = ifd0_sira;

    if u64::from(ifd0_ofseti) >= dosya_boyutu {
        return Err(Hata::IfdGecersiz {
            yol,
            ayrinti: format!(
                "IFD0 offset {} dosya siniri {} disinda",
                ifd0_ofseti, dosya_boyutu
            ),
        });
    }

    let mut ziyaret: BTreeSet<u64> = BTreeSet::new();
    let ifd0 = okuyucu.ifd_oku(u64::from(ifd0_ofseti), &mut ziyaret, 1)?;
    let exif = alt_ifd_coz(okuyucu, &ifd0, ETIKET_EXIF_IFD, &mut ziyaret, 2)?;
    let gps = alt_ifd_coz(okuyucu, &ifd0, ETIKET_GPS_IFD, &mut ziyaret, 3)?;

    Ok(TiffBelge {
        baslik: Baslik {
            sira: ifd0_sira,
            ifd0_ofseti,
            ifd1_ofseti: 0,
        },
        ifd0,
        exif,
        gps,
        okunan_bayt: okuyucu.okunan_bayt(),
        dosya_boyutu,
    })
}

/// Alt IFD işaretçisini çözer; işaretçi yoksa veya geçersizse `None` verir.
fn alt_ifd_coz(
    okuyucu: &mut TiffOkuyucu,
    ust: &Ifd,
    etiket: u16,
    ziyaret: &mut BTreeSet<u64>,
    adim: usize,
) -> Sonuc<Option<Ifd>> {
    let Some(girdi) = ust.bul(etiket) else {
        return Ok(None);
    };
    let Some(ofset) = okuyucu.deger_u32(girdi)? else {
        return Ok(None);
    };
    if ofset == 0 || u64::from(ofset) >= okuyucu.dosya_boyutu() {
        return Ok(None);
    }
    match okuyucu.ifd_oku(u64::from(ofset), ziyaret, adim) {
        Ok(ifd) => Ok(Some(ifd)),
        // Alt dizin bozuksa ana belge kaybolmaz: alan eksik sayılır.
        Err(Hata::IfdSinirAsim { .. }) | Err(Hata::IfdGecersiz { .. }) => Ok(None),
        Err(diger) => Err(diger),
    }
}

#[cfg(test)]
// Gerekçe: `unwrap`/`expect` yalnızca test içinde kullanılır ve testin
// başarısızlık mesajıdır. Üretim kodunda crate seviyesinde clippy
// lintleri açıktır.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Test dosyası yazar ve `Drop` ile siler.
    struct Gecici {
        yol: PathBuf,
    }

    impl Gecici {
        fn yeni(etiket: &str, icerik: &[u8]) -> Gecici {
            let yol =
                std::env::temp_dir().join(format!("bj-tiff-{}-{}", etiket, std::process::id()));
            std::fs::write(&yol, icerik).expect("gecici yazma");
            Gecici { yol }
        }
    }

    impl Drop for Gecici {
        fn drop(&mut self) {
            // Drop içinden hata döndürülemez; temizlik başarısız olsa da
            // testi düşürmemelidir (WORKER_CONTRACT § 5.3).
            let _ = std::fs::remove_file(&self.yol);
        }
    }

    /// Etiket girişinin 12 baytını üretir.
    fn girdi(etiket: u16, tip: u16, adet: u32, deger: [u8; 4]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&etiket.to_le_bytes());
        v.extend_from_slice(&tip.to_le_bytes());
        v.extend_from_slice(&adet.to_le_bytes());
        v.extend_from_slice(&deger);
        v
    }

    /// Değerleri satır içine sığan sade bir IFD0 dosyası kurar.
    fn ornek_tiff(girdiler: &[Vec<u8>], ek: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"II\x2a\x00");
        v.extend_from_slice(&8u32.to_le_bytes());
        let adet = girdiler.len() as u16;
        v.extend_from_slice(&adet.to_le_bytes());
        for g in girdiler {
            v.extend_from_slice(g);
        }
        v.extend_from_slice(&0u32.to_le_bytes()); // sonraki IFD yok
        v.extend_from_slice(ek);
        v
    }

    #[test]
    fn alan_tipi_kodlari_tiff_6_0_ile_uyumlu() {
        assert_eq!(AlanTipi::koddan(1), Some(AlanTipi::Bayt));
        assert_eq!(AlanTipi::koddan(5), Some(AlanTipi::Rasyonel));
        assert_eq!(AlanTipi::koddan(13), Some(AlanTipi::Ifd));
        assert_eq!(AlanTipi::koddan(99), None);
        assert_eq!(AlanTipi::Rasyonel.genislik(), 8);
        assert_eq!(AlanTipi::Short.genislik(), 2);
        assert_eq!(AlanTipi::Bayt.genislik(), 1);
    }

    #[test]
    fn bayt_sirasi_her_iki_yonu_cozer() {
        assert_eq!(BaytSirasi::Kucuk.u16([0x01, 0x02]), 0x0201);
        assert_eq!(BaytSirasi::Buyuk.u16([0x01, 0x02]), 0x0102);
        assert_eq!(BaytSirasi::Kucuk.u32([1, 0, 0, 0]), 1);
        assert_eq!(BaytSirasi::Buyuk.u32([0, 0, 0, 1]), 1);
    }

    #[test]
    fn ifd0_dizinini_cozer() {
        let ham = ornek_tiff(
            &[
                girdi(0x0112, 3, 1, [1, 0, 0, 0]),       // Orientation = 1
                girdi(0x0110, 3, 1, [0xE0, 0x08, 0, 0]), // ImageLength = 2272
            ],
            &[],
        );
        let g = Gecici::yeni("ifd0", &ham);
        let belge = coz(&g.yol).expect("coz");
        assert_eq!(belge.ifd0.girdiler.len(), 2);
        assert!(belge.ifd0.var(ETIKET_YON));
        assert!(belge.exif.is_none());
    }

    #[test]
    fn buyuk_endian_dosyayi_cozer() {
        let mut v: Vec<u8> = Vec::new();
        v.extend_from_slice(b"MM\x00\x2a");
        v.extend_from_slice(&8u32.to_be_bytes());
        v.extend_from_slice(&1u16.to_be_bytes());
        v.extend_from_slice(&0x0112u16.to_be_bytes());
        v.extend_from_slice(&3u16.to_be_bytes());
        v.extend_from_slice(&1u32.to_be_bytes());
        v.extend_from_slice(&6u16.to_be_bytes());
        v.extend_from_slice(&0u32.to_be_bytes());
        v.extend_from_slice(&0u32.to_be_bytes());
        let g = Gecici::yeni("mm", &v);
        let belge = coz(&g.yol).expect("coz");
        assert_eq!(belge.baslik.sira, BaytSirasi::Buyuk);
        let girdi = belge.ifd0.bul(ETIKET_YON).expect("yon");
        assert_eq!(girdi.adet, 1);
    }

    #[test]
    fn bigtiff_reddedilir() {
        let mut v = Vec::from(*b"II\x2b\x00");
        v.extend_from_slice(&8u32.to_le_bytes());
        v.resize(64, 0);
        let g = Gecici::yeni("bigtiff", &v);
        match coz(&g.yol) {
            Err(Hata::BigTiffReddedildi { .. }) => {}
            diger => panic!("BigTIFF reddedilmeliydi, {:?}", diger),
        }
    }

    #[test]
    fn bayt_sirasi_imzasi_gecersizse_hata_verir() {
        let v = vec![0x46u8, 0x46, 0, 42, 8, 0, 0, 0];
        let g = Gecici::yeni("imza", &v);
        match coz(&g.yol) {
            Err(Hata::TiffBazliDegil { ayrinti, .. }) => assert!(ayrinti.contains("imzası")),
            diger => panic!("TIFF-bazlı değil hatası bekleniyordu, {:?}", diger),
        }
    }

    #[test]
    fn kisa_dosya_basliksiz_sayilir() {
        let g = Gecici::yeni("kisa", b"II\x2a");
        match coz(&g.yol) {
            Err(Hata::TiffBazliDegil { .. }) => {}
            diger => panic!("kısa dosya reddedilmeliydi, {:?}", diger),
        }
    }

    #[test]
    fn girdi_sayisi_siniri_asilirsa_hata_verir() {
        let mut ham = Vec::from(*b"II\x2a\x00");
        ham.extend_from_slice(&8u32.to_le_bytes());
        ham.extend_from_slice(&(IFD_GIRD_LIMITI as u16 + 1).to_le_bytes());
        ham.resize(16 + (IFD_GIRD_LIMITI + 1) * 12, 0);
        let g = Gecici::yeni("cokgirdi", &ham);
        match coz(&g.yol) {
            Err(Hata::IfdGecersiz { ayrinti, .. }) => assert!(ayrinti.contains("sınırı")),
            diger => panic!("girdi sınırı bekleniyordu, {:?}", diger),
        }
    }

    #[test]
    fn offset_dosya_sini_disinda_hata_verir() {
        let mut ham = Vec::from(*b"II\x2a\x00");
        ham.extend_from_slice(&900_000u32.to_le_bytes());
        let g = Gecici::yeni("uzakoffset", &ham);
        match coz(&g.yol) {
            Err(Hata::IfdGecersiz { ayrinti, .. }) => assert!(ayrinti.contains("siniri")),
            diger => panic!("kapsam dışı offset bekleniyordu, {:?}", diger),
        }
    }

    #[test]
    fn offset_dongusu_koruma_tetikler() {
        // IFD0 kendi kendine işaret eder: ikinci kez aynı offset görülür.
        let mut ham = Vec::from(*b"II\x2a\x00");
        ham.extend_from_slice(&8u32.to_le_bytes());
        ham.extend_from_slice(&1u16.to_le_bytes());
        // Exif IFD pointer = 8 (IFD0'ın kendi adresi)
        ham.extend_from_slice(&ETIKET_EXIF_IFD.to_le_bytes());
        ham.extend_from_slice(&4u16.to_le_bytes());
        ham.extend_from_slice(&1u32.to_le_bytes());
        ham.extend_from_slice(&8u32.to_le_bytes());
        ham.extend_from_slice(&0u32.to_le_bytes());
        let g = Gecici::yeni("dongu", &ham);
        // IFD0 çözülür, Exif işaretçisi ziyaret edilmiş offset'e döner -> None.
        let belge = coz(&g.yol).expect("coz");
        assert!(belge.exif.is_none(), "döngü alt dizin olarak alınmamalı");
    }

    #[test]
    fn ayni_offset_iki_kez_ifd_oku_cagrisinda_hata_verir() {
        let ham = ornek_tiff(&[girdi(0x0112, 3, 1, [1, 0, 0, 0])], &[]);
        let g = Gecici::yeni("ziyaret", &ham);
        let (mut okuyucu, baslik) = TiffOkuyucu::ac(&g.yol).expect("ac");
        let mut ziyaret = BTreeSet::new();
        okuyucu
            .ifd_oku(u64::from(baslik.ifd0_ofseti), &mut ziyaret, 1)
            .expect("ilk ifd");
        match okuyucu.ifd_oku(u64::from(baslik.ifd0_ofseti), &mut ziyaret, 2) {
            Err(Hata::IfdGecersiz { ayrinti, .. }) => assert!(ayrinti.contains("döngüsü")),
            diger => panic!("döngü hatası bekleniyordu, {:?}", diger),
        }
    }

    #[test]
    fn adim_siniri_koruma_tetikler() {
        let ham = ornek_tiff(&[girdi(0x0112, 3, 1, [1, 0, 0, 0])], &[]);
        let g = Gecici::yeni("adim", &ham);
        let (mut okuyucu, baslik) = TiffOkuyucu::ac(&g.yol).expect("ac");
        let mut ziyaret = BTreeSet::new();
        match okuyucu.ifd_oku(
            u64::from(baslik.ifd0_ofseti),
            &mut ziyaret,
            IFD_ADIM_LIMITI + 1,
        ) {
            Err(Hata::IfdSinirAsim { adim, .. }) => assert_eq!(adim, IFD_ADIM_LIMITI + 1),
            diger => panic!("adım sınırı bekleniyordu, {:?}", diger),
        }
    }

    #[test]
    fn satir_ici_deger_dosyadan_okunmaz() {
        let ham = ornek_tiff(&[girdi(0x0112, 3, 1, [6, 0, 0, 0])], &[]);
        let g = Gecici::yeni("satirici", &ham);
        let belge = coz(&g.yol).expect("coz");
        let mut okuyucu = TiffOkuyucu::ac(&g.yol).expect("ac").0;
        let girdi = belge.ifd0.bul(ETIKET_YON).expect("yon");
        assert!(girdi.satir_ici_mi());
        assert_eq!(okuyucu.deger_u16(girdi).expect("deger"), Some(6));
    }

    #[test]
    fn satir_disi_deger_dosyadan_okunur() {
        // ImageWidth SHORT değil, ASCII (tip 2) 12 bayt: offset gerekir.
        let mut ham = Vec::new();
        ham.extend_from_slice(b"II\x2a\x00");
        ham.extend_from_slice(&8u32.to_le_bytes());
        ham.extend_from_slice(&1u16.to_le_bytes());
        ham.extend_from_slice(&0x010Eu16.to_le_bytes());
        ham.extend_from_slice(&2u16.to_le_bytes());
        ham.extend_from_slice(&12u32.to_le_bytes());
        let deger_ofseti = 8u32 + 2 + 12 + 4;
        ham.extend_from_slice(&deger_ofseti.to_le_bytes());
        ham.extend_from_slice(&0u32.to_le_bytes());
        ham.extend_from_slice(b"Test Kamera\0\0");
        let g = Gecici::yeni("metin", &ham);
        let belge = coz(&g.yol).expect("coz");
        let mut okuyucu = TiffOkuyucu::ac(&g.yol).expect("ac").0;
        let girdi = belge.ifd0.bul(ETIKET_ACIKLAMA).expect("aciklama");
        assert!(!girdi.satir_ici_mi());
        assert_eq!(
            okuyucu.deger_metin(girdi).expect("metin").as_deref(),
            Some("Test Kamera")
        );
    }

    #[test]
    fn rasyonel_deger_ondelik_bolunur() {
        // FNumber = 28/10 => 2.8
        let mut ham = Vec::new();
        ham.extend_from_slice(b"II\x2a\x00");
        ham.extend_from_slice(&8u32.to_le_bytes());
        ham.extend_from_slice(&1u16.to_le_bytes());
        ham.extend_from_slice(&ETIKET_DIYAFRAM.to_le_bytes());
        ham.extend_from_slice(&5u16.to_le_bytes());
        ham.extend_from_slice(&1u32.to_le_bytes());
        let ofset = 8u32 + 2 + 12 + 4;
        ham.extend_from_slice(&ofset.to_le_bytes());
        ham.extend_from_slice(&0u32.to_le_bytes());
        ham.extend_from_slice(&28u32.to_le_bytes());
        ham.extend_from_slice(&10u32.to_le_bytes());
        let g = Gecici::yeni("rasyonel", &ham);
        let belge = coz(&g.yol).expect("coz");
        let mut okuyucu = TiffOkuyucu::ac(&g.yol).expect("ac").0;
        let girdi = belge.ifd0.bul(ETIKET_DIYAFRAM).expect("diyafram");
        let deger = okuyucu.deger_f32(girdi).expect("deger").expect("biri");
        assert!((deger - 2.8).abs() < 1e-5, "{}", deger);
    }

    #[test]
    fn paydasi_sifir_rasyonel_none_doner() {
        let mut ham = Vec::new();
        ham.extend_from_slice(b"II\x2a\x00");
        ham.extend_from_slice(&8u32.to_le_bytes());
        ham.extend_from_slice(&1u16.to_le_bytes());
        ham.extend_from_slice(&ETIKET_DIYAFRAM.to_le_bytes());
        ham.extend_from_slice(&5u16.to_le_bytes());
        ham.extend_from_slice(&1u32.to_le_bytes());
        let ofset = 8u32 + 2 + 12 + 4;
        ham.extend_from_slice(&ofset.to_le_bytes());
        ham.extend_from_slice(&0u32.to_le_bytes());
        ham.extend_from_slice(&5u32.to_le_bytes());
        ham.extend_from_slice(&0u32.to_le_bytes());
        let g = Gecici::yeni("sifirpayda", &ham);
        let belge = coz(&g.yol).expect("coz");
        let mut okuyucu = TiffOkuyucu::ac(&g.yol).expect("ac").0;
        let girdi = belge.ifd0.bul(ETIKET_DIYAFRAM).expect("diyafram");
        assert!(okuyucu.deger_f32(girdi).expect("deger").is_none());
    }

    #[test]
    fn deger_ust_siniri_asilirsa_okunmaz() {
        let mut ham = Vec::new();
        ham.extend_from_slice(b"II\x2a\x00");
        ham.extend_from_slice(&8u32.to_le_bytes());
        ham.extend_from_slice(&1u16.to_le_bytes());
        // 9000 baytlık Undefined blob: sınırı aşıyor.
        ham.extend_from_slice(&0x9999u16.to_le_bytes());
        ham.extend_from_slice(&7u16.to_le_bytes());
        ham.extend_from_slice(&(DEGER_LIMITI as u32 + 1).to_le_bytes());
        ham.extend_from_slice(&64u32.to_le_bytes());
        ham.extend_from_slice(&0u32.to_le_bytes());
        let g = Gecici::yeni("buyukdeger", &ham);
        let belge = coz(&g.yol).expect("coz");
        let mut okuyucu = TiffOkuyucu::ac(&g.yol).expect("ac").0;
        let girdi = belge.ifd0.bul(0x9999).expect("girdi");
        match okuyucu.deger(girdi) {
            Err(Hata::IfdGecersiz { ayrinti, .. }) => assert!(ayrinti.contains("sınırı")),
            diger => panic!("değer sınırı bekleniyordu, {:?}", diger),
        }
    }

    #[test]
    fn bilinmeyen_tip_dizini_bozmaz() {
        let ham = ornek_tiff(
            &[
                girdi(0x9999, 42, 1, [0, 0, 0, 0]), // bilinmeyen tip kodu
                girdi(ETIKET_YON, 3, 1, [1, 0, 0, 0]),
            ],
            &[],
        );
        let g = Gecici::yeni("bilinmeyentip", &ham);
        let belge = coz(&g.yol).expect("coz");
        assert_eq!(belge.ifd0.girdiler.len(), 2);
        assert!(belge.ifd0.var(ETIKET_YON));
    }

    #[test]
    fn exif_ifd_isaretcisi_cozulur() {
        // IFD0 (8) + 18 bayt -> Exif IFD 26'da başlar.
        let mut ham = Vec::new();
        ham.extend_from_slice(b"II\x2a\x00");
        ham.extend_from_slice(&8u32.to_le_bytes());
        ham.extend_from_slice(&1u16.to_le_bytes());
        ham.extend_from_slice(&ETIKET_EXIF_IFD.to_le_bytes());
        ham.extend_from_slice(&4u16.to_le_bytes());
        ham.extend_from_slice(&1u32.to_le_bytes());
        ham.extend_from_slice(&26u32.to_le_bytes());
        ham.extend_from_slice(&0u32.to_le_bytes());
        // Exif IFD (26): 1 girdi (ISO = 400), sonra 0
        ham.extend_from_slice(&1u16.to_le_bytes());
        ham.extend_from_slice(&ETIKET_ISO.to_le_bytes());
        ham.extend_from_slice(&3u16.to_le_bytes());
        ham.extend_from_slice(&1u32.to_le_bytes());
        ham.extend_from_slice(&400u16.to_le_bytes());
        ham.extend_from_slice(&0u16.to_le_bytes());
        ham.extend_from_slice(&0u32.to_le_bytes());
        let g = Gecici::yeni("exif", &ham);
        let belge = coz(&g.yol).expect("coz");
        let exif = belge.exif.as_ref().expect("exif ifd");
        assert!(exif.var(ETIKET_ISO));
        assert!(belge.etiket(ETIKET_ISO).is_some());
    }

    #[test]
    fn okunan_bayt_dosya_boyutundan_kucuktur() {
        // 400 KiB'lık sahte dosyanın yalnız dizini okunmalı.
        let ham = ornek_tiff(&[girdi(0x0112, 3, 1, [1, 0, 0, 0])], &vec![0u8; 400 * 1024]);
        let g = Gecici::yeni("kismi", &ham);
        let belge = coz(&g.yol).expect("coz");
        assert!(belge.okunan_bayt < 1024, "{}", belge.okunan_bayt);
        assert!(belge.okuma_orani() < 0.01, "{}", belge.okuma_orani());
        assert_eq!(belge.dosya_boyutu, ham.len() as u64);
    }

    #[test]
    fn bos_dizin_gecerli_belgedir() {
        let ham = ornek_tiff(&[], &[]);
        let g = Gecici::yeni("bosifd", &ham);
        let belge = coz(&g.yol).expect("coz");
        assert!(belge.ifd0.girdiler.is_empty());
        assert!(belge.exif.is_none());
    }

    #[test]
    fn okuma_orani_bos_dosyada_sifirdir() {
        let belge = TiffBelge {
            baslik: Baslik {
                sira: BaytSirasi::Kucuk,
                ifd0_ofseti: 0,
                ifd1_ofseti: 0,
            },
            ifd0: Ifd::default(),
            exif: None,
            gps: None,
            okunan_bayt: 0,
            dosya_boyutu: 0,
        };
        assert_eq!(belge.okuma_orani(), 0.0);
    }
}
