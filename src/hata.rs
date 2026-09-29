//! Hata tipi ve sonuç takma adı.
//!
//! Bu modül yalnızca hata taşır: hiçbir dosya açmaz, hiçbir bayt okumaz.
//! Tarama katmanı üç sınıf hatayı birbirinden ayırır (rapor `b07`):
//!
//! 1. **Okunamayan dosya** — taramayı durdurmaz, atlanır ve `Hata::Okunamadi`
//!    olarak raporlanır.
//! 2. **Önizleme üretim hatası** — dosya yine de metadatasıyla listelenir,
//!    yalnızca hash boş kalır (`Hata::OnizlemeBozuk`).
//! 3. **Komut satırı / ayar hatası** — programı durdurur (`Hata::Parametre`).

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// Tüm genel işlemlerin sonuç tipi.
pub type Sonuc<T> = Result<T, Hata>;

/// BurstJudge'in ürettiği hataların tamamı.
#[derive(Debug)]
#[non_exhaustive]
pub enum Hata {
    /// Dosya sistemi işlemi başarısız oldu.
    Io {
        /// Yapılmak istenen işlem ("dizin tarama", "önizleme yaz" ...).
        eylem: &'static str,
        /// İşlemin konusu olan yol.
        yol: PathBuf,
        /// Altta yatan işletim sistemi hatası.
        kaynak: io::Error,
    },
    /// Tarama kökü bir dizin değil.
    TaramaYoluHatali {
        /// Verilen yol.
        yol: PathBuf,
        /// Neden uygun olmadığı.
        ayrinti: String,
    },
    /// TIFF başlığı tanınmadı ya da dosya TIFF-tabanlı değil.
    TiffBazliDegil {
        /// İncelenen dosya.
        yol: PathBuf,
        /// Neden TIFF sayılmadığı.
        ayrinti: String,
    },
    /// BigTIFF imzası bulundu; bu MVP bilinçli olarak desteklenmiyor.
    BigTiffReddedildi {
        /// Reddedilen dosya.
        yol: PathBuf,
    },
    /// IFD girdisi geçersiz: sayım, tip, uzunluk veya offset kapsam dışı.
    IfdGecersiz {
        /// İncelenen dosya.
        yol: PathBuf,
        /// Hatanın ayrıntısı.
        ayrinti: String,
    },
    /// IFD zinciri koruma sınırına takıldı: sonsuz döngü olasılığı.
    IfdSinirAsim {
        /// Korumanın devreye girdiği dosya.
        yol: PathBuf,
        /// Kaç adım sonra durulduğu.
        adim: usize,
    },
    /// Gömülü JPEG önizleme bulunamadı.
    OnizlemeYok {
        /// Önizleme aranan dosya.
        yol: PathBuf,
    },
    /// JPEG önizleme bulundu ama çözülemedi.
    OnizlemeBozuk {
        /// Önizlemesi bozuk olan dosya.
        yol: PathBuf,
        /// Kod çözücünün verdiği açıklama.
        ayrinti: String,
    },
    /// Ayarlar dosyası okunamadı veya şemaya uymadı.
    AyarGecersiz {
        /// Ayar dosyasının yolu.
        dosya: PathBuf,
        /// Ayrıntılı açıklama.
        ayrinti: String,
    },
    /// Dışa aktarma hedefi kaynak klasörle aynı veya bir alt klasörü.
    HedefCakisti {
        /// Reddedilen hedef.
        yol: PathBuf,
    },
    /// Komut satırından gelen eksik veya tutarsız parametre.
    Parametre {
        /// Sorunlu parametrenin adı.
        ad: &'static str,
        /// Açıklama.
        ayrinti: String,
    },
}

impl Hata {
    /// Dosya sistemi hatasını yol ve eylem bağlamıyla sarar.
    pub fn io(eylem: &'static str, yol: &Path, kaynak: io::Error) -> Self {
        Hata::Io {
            eylem,
            yol: yol.to_path_buf(),
            kaynak,
        }
    }

    /// Parametre hatası için kısa bir kurucu.
    pub fn parametre(ad: &'static str, ayrinti: impl Into<String>) -> Self {
        Hata::Parametre {
            ad,
            ayrinti: ayrinti.into(),
        }
    }

    /// Kullanıcıya gösterilecek tek satırlık özet.
    ///
    /// Tarama döngüsü bu metni `okunamayan dosyalar` listesine yazar; hatayı
    /// ayrıntılarıyla basmak yerine kısa bir etiket yeterlidir.
    pub fn etiket(&self) -> &'static str {
        match self {
            Hata::Io { .. } => "io",
            Hata::TaramaYoluHatali { .. } => "tarama-yolu",
            Hata::TiffBazliDegil { .. } => "tiff-bazli-degil",
            Hata::BigTiffReddedildi { .. } => "bigtiff",
            Hata::IfdGecersiz { .. } => "ifd-gecersiz",
            Hata::IfdSinirAsim { .. } => "ifd-sinir",
            Hata::OnizlemeYok { .. } => "onizleme-yok",
            Hata::OnizlemeBozuk { .. } => "onizleme-bozuk",
            Hata::AyarGecersiz { .. } => "ayar-gecersiz",
            Hata::HedefCakisti { .. } => "hedef-cakisti",
            Hata::Parametre { .. } => "parametre",
        }
    }
}

impl fmt::Display for Hata {
    fn fmt(&self, bicik: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Hata::Io { eylem, yol, kaynak } => {
                write!(bicik, "{} başarısız ({}): {}", eylem, yol.display(), kaynak)
            }
            Hata::TaramaYoluHatali { yol, ayrinti } => {
                write!(
                    bicik,
                    "tarama yolu geçersiz ({}): {}",
                    yol.display(),
                    ayrinti
                )
            }
            Hata::TiffBazliDegil { yol, ayrinti } => {
                write!(bicik, "TIFF-tabanlı değil ({}): {}", yol.display(), ayrinti)
            }
            Hata::BigTiffReddedildi { yol } => write!(
                bicik,
                "BigTIFF reddedildi ({}): sürüm 42 kapsam dışı",
                yol.display()
            ),
            Hata::IfdGecersiz { yol, ayrinti } => {
                write!(bicik, "IFD geçersiz ({}): {}", yol.display(), ayrinti)
            }
            Hata::IfdSinirAsim { yol, adim } => write!(
                bicik,
                "IFD zinciri koruma sınırına takıldı ({}): {} adımdan sonra duruldu",
                yol.display(),
                adim
            ),
            Hata::OnizlemeYok { yol } => {
                write!(bicik, "gömülü önizleme yok: {}", yol.display())
            }
            Hata::OnizlemeBozuk { yol, ayrinti } => {
                write!(
                    bicik,
                    "önizleme çözülemedi ({}): {}",
                    yol.display(),
                    ayrinti
                )
            }
            Hata::AyarGecersiz { dosya, ayrinti } => {
                write!(
                    bicik,
                    "ayar dosyası geçersiz ({}): {}",
                    dosya.display(),
                    ayrinti
                )
            }
            Hata::HedefCakisti { yol } => write!(
                bicik,
                "dışa aktarma hedefi kaynak klasörün içinde olamaz: {}",
                yol.display()
            ),
            Hata::Parametre { ad, ayrinti } => {
                write!(bicik, "parametre hatası ({}): {}", ad, ayrinti)
            }
        }
    }
}

impl std::error::Error for Hata {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Hata::Io { kaynak, .. } => Some(kaynak),
            _ => None,
        }
    }
}

#[cfg(test)]
// Gerekçe: `expect` yalnızca test içinde kullanılır ve testin başarısızlık
// mesajıdır. Üretim kodunda crate seviyesinde clippy::unwrap_used/expect_used
// uyarıları açıktır.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn gecici_yol(ad: &str) -> PathBuf {
        std::env::temp_dir().join(ad)
    }

    #[test]
    fn io_hatasi_eylem_ve_yol_tasir() {
        let kaynak = io::Error::new(io::ErrorKind::NotFound, "dosya yok");
        let h = Hata::io("dizin tarama", &gecici_yol("ornek"), kaynak);
        let metin = h.to_string();
        assert!(metin.contains("dizin tarama"), "{}", metin);
        assert!(metin.contains("ornek"), "{}", metin);
    }

    #[test]
    fn io_hatasi_kaynak_zinciri_bildirir() {
        let kaynak = io::Error::new(io::ErrorKind::PermissionDenied, "izin yok");
        let h = Hata::io("klasör okuma", &gecici_yol("k"), kaynak);
        assert!(std::error::Error::source(&h).is_some());
    }

    #[test]
    fn bigtiff_reddi_mesajinda_dosya_yolu_gecer() {
        let h = Hata::BigTiffReddedildi {
            yol: gecici_yol("buyuk.dng"),
        };
        let metin = h.to_string();
        assert!(metin.contains("BigTIFF"), "{}", metin);
        assert!(metin.contains("buyuk.dng"), "{}", metin);
    }

    #[test]
    fn ifd_sinir_hatasi_adimi_yazar() {
        let h = Hata::IfdSinirAsim {
            yol: gecici_yol("dongu.tif"),
            adim: 64,
        };
        let metin = h.to_string();
        assert!(metin.contains("64"), "{}", metin);
    }

    #[test]
    fn parametre_hatasi_kurucusu_ayrinti_yazar() {
        let h = Hata::parametre("esik", "0..64 aralığında olmalı");
        assert!(h.to_string().contains("0..64"));
    }

    #[test]
    fn tum_cesitler_ayri_etiket_dondurur() {
        let cesitler = [
            Hata::io("x", &gecici_yol("a"), io::Error::other("b")),
            Hata::TaramaYoluHatali {
                yol: gecici_yol("a"),
                ayrinti: "b".into(),
            },
            Hata::TiffBazliDegil {
                yol: gecici_yol("a"),
                ayrinti: "b".into(),
            },
            Hata::BigTiffReddedildi {
                yol: gecici_yol("a"),
            },
            Hata::IfdGecersiz {
                yol: gecici_yol("a"),
                ayrinti: "b".into(),
            },
            Hata::IfdSinirAsim {
                yol: gecici_yol("a"),
                adim: 1,
            },
            Hata::OnizlemeYok {
                yol: gecici_yol("a"),
            },
            Hata::OnizlemeBozuk {
                yol: gecici_yol("a"),
                ayrinti: "b".into(),
            },
            Hata::AyarGecersiz {
                dosya: gecici_yol("a"),
                ayrinti: "b".into(),
            },
            Hata::HedefCakisti {
                yol: gecici_yol("a"),
            },
            Hata::parametre("a", "b"),
        ];
        let etiketler: Vec<&str> = cesitler.iter().map(Hata::etiket).collect();
        // Her çeşit kendi etiketini dönmeli; hepsi birbirinden farklı olmalı.
        let benzersiz: std::collections::BTreeSet<&&str> = etiketler.iter().collect();
        assert_eq!(benzersiz.len(), cesitler.len(), "{:?}", etiketler);
    }

    #[test]
    fn onizleme_yok_hatasi_yolu_yazar() {
        let h = Hata::OnizlemeYok {
            yol: gecici_yol("gorsel.jpg"),
        };
        assert!(h.to_string().contains("gorsel.jpg"));
    }
}
