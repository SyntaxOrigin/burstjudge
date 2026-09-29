//! BurstJudge komut satırı arayüzü.
//!
//! Alt komutlar:
//!
//! - `group`  — tara, grupla, özet göster (JSON çıktı isteğe bağlı).
//! - `judge`  — tara, grupla, puanla, her grupta en iyi kareyi öner (JSON).
//! - `export` — rapordaki önerileri yeni klasöre kopyalar, kaynağa dokunmaz.
//! - `info`   — tek bir dosyanın IFD/EXIF özetini yazar.
//!
//! Aritmetik: `toplam okunan bayt / toplam dosya boyutu` oranı her komutta
//! raporlanır; bu, "hiçbir dosya tam çözülmez" iddiasının ölçülebilir kanıtıdır.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use burstjudge::aktar;
use burstjudge::bhash::AlgisalHash;
use burstjudge::grupla::{grupla, Oge};
use burstjudge::hata::{Hata, Sonuc};
use burstjudge::motor::{AtlananDosya, KareKaydi};
use burstjudge::puan::Ayarlar;
use burstjudge::rapor::{self, Rapor};
use burstjudge::{motor::Motor, tiff, SURUM};

use clap::{Args, Parser, Subcommand};

/// BurstJudge — burst karelerini gruplayan ve en iyisini öneren terminal aracı.
#[derive(Parser, Debug)]
#[command(name = "burstjudge", version, about, long_about = None)]
struct Cli {
    /// Hangi işlem yapılacak.
    #[command(subcommand)]
    komut: Komut,
}

/// Alt komutlar.
#[derive(Subcommand, Debug)]
enum Komut {
    /// Klasörü tara, benzer kareleri grupla ve özet yaz.
    Group(GroupArg),
    /// Klasörü tara, puanla ve her grupta en iyi kareyi öner.
    Judge(JudgeArg),
    /// Rapor önerilerini yeni klasöre kopyala (kaynak değişmez).
    Export(ExportArg),
    /// Tek bir dosyanın IFD/EXIF özetini yaz.
    Info(InfoArg),
}

/// `group` alt komutunun argümanları.
#[derive(Args, Debug)]
struct GroupArg {
    /// Taranacak klasör.
    kok: PathBuf,
    /// Hamming mesafesi eşiği (0-64). Varsayılan: 8.
    #[arg(short, long, default_value_t = 8)]
    esik: u32,
    /// JSON raporunun yazılacağı dosya. Verilmezse stdout'a özet yazılır.
    #[arg(short, long)]
    cikti: Option<PathBuf>,
    /// Algısal hash hesaplanmadan yalnız metadatası taranır (önizleme okunmaz).
    #[arg(long)]
    hash_yok: bool,
    /// Yalnız önizleme bulunan dosyalar sayılsın.
    #[arg(long, default_value_t = false)]
    onizleme_istatistik: bool,
}

/// `judge` alt komutunun argümanları.
#[derive(Args, Debug)]
struct JudgeArg {
    /// Taranacak klasör.
    kok: PathBuf,
    /// Hamming mesafesi eşiği (0-64). Varsayılan: 8.
    #[arg(short, long, default_value_t = 8)]
    esik: u32,
    /// JSON raporunun yazılacağı dosya. Verilmezse JSON stdout'a yazılır.
    #[arg(short, long)]
    cikti: Option<PathBuf>,
    /// Puan ağırlıklarının okunacağı JSON ayar dosyası.
    #[arg(long)]
    ayar: Option<PathBuf>,
}

/// `export` alt komutunun argümanları.
#[derive(Args, Debug)]
struct ExportArg {
    /// Önceki `judge` çıktısı olan JSON rapor dosyası.
    rapor: PathBuf,
    /// Karelerin kopyalanacağı hedef klasör (kaynakla çakışmamalı).
    hedef: PathBuf,
    /// Yalnız bu grup kimliği dışa aktarılır (verilmezse tüm gruplar).
    #[arg(short, long)]
    grup: Option<usize>,
}

/// `info` alt komutunun argümanları.
#[derive(Args, Debug)]
struct InfoArg {
    /// İncelenecek dosya.
    dosya: PathBuf,
    /// Gömülü önizlemeyi ayrı `.jpg` olarak yaz.
    #[arg(long)]
    onizleme_yaz: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let sonuc = match cli.komut {
        Komut::Group(a) => group_calistir(a),
        Komut::Judge(a) => judge_calistir(a),
        Komut::Export(a) => export_calistir(a),
        Komut::Info(a) => info_calistir(a),
    };
    match sonuc {
        Ok(()) => ExitCode::SUCCESS,
        Err(hata) => {
            eprintln!("hata: {}", hata);
            ExitCode::FAILURE
        }
    }
}

/// `group` komutunu çalıştırır.
fn group_calistir(a: GroupArg) -> Sonuc<()> {
    if a.esik > 64 {
        return Err(Hata::parametre(
            "esik",
            "hamming mesafesi 0-64 aralığında olmalı",
        ));
    }
    let motor = Motor {
        ayarlar: Ayarlar::varsayilan(),
        hash_uret: !a.hash_yok,
    };
    let (kayitlar, atlanan) = motor.tara_ayrinti(&a.kok)?;
    let istatistik = burstjudge::motor::istatistik(&kayitlar, &atlanan);

    // Hash'li kareleri grupla.
    let oge_list: Vec<Oge> = kayitlar
        .iter()
        .enumerate()
        .filter_map(|(i, k)| {
            k.hash.map(|h| Oge {
                sira: i,
                hash: AlgisalHash(h.0),
            })
        })
        .collect();
    let gruplama = grupla(&oge_list, a.esik);

    println!("BurstJudge {} — grup taraması", SURUM);
    println!("  kök           : {}", a.kok.display());
    println!("  eşik          : {}", a.esik);
    println!("  dosya         : {}", istatistik.bulunan_dosya);
    println!(
        "  okunan bayt   : {} / {} ({:.4}%)",
        istatistik.toplam_okunan_bayt,
        istatistik.toplam_dosya_boyutu,
        istatistik.okuma_orani() * 100.0
    );
    println!(
        "  önizleme      : {} bulundu, {} yok",
        istatistik.onizleme_bulunan, istatistik.onizleme_yok
    );
    println!("  hash          : {} üretildi", istatistik.hash_uretilen);
    println!("  atlanan       : {}", atlanan.len());
    println!("  grup          : {}", gruplama.grup_sayisi());
    println!("  çakışmalı grup: {}", cakismali_grup_sayisi(&gruplama));

    if a.onizleme_istatistik {
        for k in atlanan.iter().take(5) {
            println!("    atlandı: {} ({})", k.yol.display(), k.hata);
        }
    }

    if let Some(cikti) = &a.cikti {
        let ayarlar = Ayarlar::varsayilan();
        let r = rapor::olustur(
            &a.kok, &motor, a.esik, &kayitlar, &atlanan, &gruplama, &ayarlar,
        );
        let json = r.json_uret().map_err(|e| Hata::Parametre {
            ad: "cikti",
            ayrinti: format!("JSON üretilemedi: {}", e),
        })?;
        std::fs::write(cikti, json).map_err(|k| Hata::io("rapor yazma", cikti, k))?;
        println!("  rapor         : {}", cikti.display());
    }
    Ok(())
}

/// `judge` komutunu çalıştırır ve JSON rapor üretir.
fn judge_calistir(a: JudgeArg) -> Sonuc<()> {
    if a.esik > 64 {
        return Err(Hata::parametre(
            "esik",
            "hamming mesafesi 0-64 aralığında olmalı",
        ));
    }
    let ayarlar = ayar_yukle(a.ayar.as_deref())?;
    let motor = Motor {
        ayarlar: ayarlar.clone(),
        hash_uret: true,
    };
    let (kayitlar, atlanan) = motor.tara_ayrinti(&a.kok)?;
    let oge_list: Vec<Oge> = kayitlar
        .iter()
        .enumerate()
        .filter_map(|(i, k)| {
            k.hash.map(|h| Oge {
                sira: i,
                hash: AlgisalHash(h.0),
            })
        })
        .collect();
    let gruplama = grupla(&oge_list, a.esik);
    let r = rapor::olustur(
        &a.kok, &motor, a.esik, &kayitlar, &atlanan, &gruplama, &ayarlar,
    );
    let json = r.json_uret().map_err(|e| Hata::Parametre {
        ad: "cikti",
        ayrinti: format!("JSON üretilemedi: {}", e),
    })?;
    match &a.cikti {
        Some(p) => {
            std::fs::write(p, &json).map_err(|k| Hata::io("rapor yazma", p, k))?;
            eprintln!("rapor yazıldı: {}", p.display());
        }
        None => println!("{}", json),
    }
    Ok(())
}

/// `export` komutunu çalıştırır.
fn export_calistir(a: ExportArg) -> Sonuc<()> {
    let metin =
        std::fs::read_to_string(&a.rapor).map_err(|k| Hata::io("rapor okuma", &a.rapor, k))?;
    let r: Rapor = serde_json::from_str(&metin).map_err(|e| Hata::Parametre {
        ad: "rapor",
        ayrinti: format!("JSON ayrıştırılamadı: {}", e),
    })?;
    aktar::hedef_uygun(&r.kok, &a.hedef)?;
    std::fs::create_dir_all(&a.hedef)
        .map_err(|k| Hata::io("hedef dizin oluşturma", &a.hedef, k))?;

    let mut toplam = 0usize;
    for g in &r.gruplar {
        if let Some(sadece) = a.grup {
            if g.kimlik != sadece {
                continue;
            }
        }
        let hedef = aktar::benzersiz_ad(
            &a.hedef.join(
                g.oneri
                    .file_name()
                    .map(|x| x.to_os_string())
                    .unwrap_or_default(),
            ),
        );
        let bayt = aktar::kopyala(&g.oneri, &hedef)?;
        println!(
            "grup {:>3} -> {} ({} bayt, puan {:.1})",
            g.kimlik,
            hedef.display(),
            bayt,
            g.uyeler.first().map(|u| u.toplam_puan).unwrap_or(0.0)
        );
        toplam += 1;
    }
    println!("toplam {} kare dışa aktarıldı", toplam);
    Ok(())
}

/// `info` komutunu çalıştırır.
fn info_calistir(a: InfoArg) -> Sonuc<()> {
    let motor = Motor::yeni();
    let kayit = motor.dosya_isle(&a.dosya)?;
    println!("dosya        : {}", kayit.yol.display());
    println!("boyut        : {} bayt", kayit.dosya_boyutu);
    println!(
        "okunan       : {} bayt ({:.4}%)",
        kayit.okunan_bayt,
        if kayit.dosya_boyutu == 0 {
            0.0
        } else {
            kayit.okunan_bayt as f64 * 100.0 / kayit.dosya_boyutu as f64
        }
    );
    println!(
        "kamera       : {} {}",
        kayit.meta.make.clone().unwrap_or_else(|| "-".into()),
        kayit.meta.model.clone().unwrap_or_else(|| "-".into())
    );
    println!(
        "çekim        : {}",
        kayit.meta.zaman.clone().unwrap_or_else(|| "-".into())
    );
    println!(
        "pozlama      : 1/{:.0} f/{} ISO {}",
        kayit.meta.poz_suresi.map(|p| 1.0 / p).unwrap_or(0.0),
        kayit
            .meta
            .diyafram
            .map(|d| format!("{:.1}", d))
            .unwrap_or_else(|| "-".into()),
        kayit
            .meta
            .iso
            .map(|i| i.to_string())
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "odak         : {} mm",
        kayit
            .meta
            .odak_mm
            .map(|o| format!("{:.0}", o))
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "boyut/yön    : {}x{} yön {:?}",
        kayit.meta.genislik.unwrap_or(0),
        kayit.meta.yukseklik.unwrap_or(0),
        kayit.meta.yon_kodu
    );
    println!("en-boy       : {:?}", kayit.meta.en_boy_orani);
    println!(
        "önizleme     : {} ({:?} bayt)",
        if kayit.onizleme_var { "var" } else { "yok" },
        kayit.onizleme_bayt
    );
    println!(
        "hash         : {}",
        kayit
            .hash
            .map(|h| h.onaltilik())
            .unwrap_or_else(|| "-".into())
    );
    println!("puan         : {:.1}", kayit.puan.toplam);
    for b in &kayit.puan.bilesenler {
        println!(
            "  - {:<12} {:>6}  (ağırlık {:.2}) {}",
            b.ad,
            b.puan
                .map(|v| format!("{:.1}", v))
                .unwrap_or_else(|| "-".into()),
            b.agirlik,
            b.gerekce
        );
    }

    if let Some(hedef) = &a.onizleme_yaz {
        if let Some(hedef) = hedef.parent().filter(|p| !p.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(hedef);
        }
        // Önizlemeyi ayrı dosyaya yazmak için konumu yeniden bul.
        onizleme_disa_yaz(&a.dosya, hedef)?;
        println!("önizleme yazıldı: {}", hedef.display());
    }
    Ok(())
}

/// Gömülü önizlemeyi hedef dosyaya bayt bayt yazar.
fn onizleme_disa_yaz(kaynak: &Path, hedef: &Path) -> Sonuc<()> {
    use std::fs::File;
    let (mut okuyucu, _baslik) = tiff::TiffOkuyucu::ac(kaynak)?;
    let belge = tiff::coz_oku(&mut okuyucu)?;
    let konum = burstjudge::onizleme::onizleme_konumu(&mut okuyucu, &belge)?;
    let mut hedef_dosya =
        File::create(hedef).map_err(|k| Hata::io("önizleme oluşturma", hedef, k))?;
    let yazilan = burstjudge::onizleme::onizleme_yaz(&mut okuyucu, &konum, &mut hedef_dosya)?;
    hedef_dosya
        .sync_all()
        .map_err(|k| Hata::io("önizleme boşaltma", hedef, k))?;
    println!("  {} bayt yazıldı", yazilan);
    Ok(())
}

/// Ayar dosyasını yükler; verilmezse varsayılanları döndürür.
fn ayar_yukle(yol: Option<&Path>) -> Sonuc<Ayarlar> {
    let Some(p) = yol else {
        return Ok(Ayarlar::varsayilan());
    };
    let metin = std::fs::read_to_string(p).map_err(|k| Hata::io("ayar okuma", p, k))?;
    let a: Ayarlar = serde_json::from_str(&metin).map_err(|e| Hata::AyarGecersiz {
        dosya: p.to_path_buf(),
        ayrinti: e.to_string(),
    })?;
    a.dogrula().map_err(|e| Hata::AyarGecersiz {
        dosya: p.to_path_buf(),
        ayrinti: e,
    })?;
    Ok(a)
}

/// Çakışma içeren grup sayısını hesaplar.
fn cakismali_grup_sayisi(g: &burstjudge::Gruplama) -> usize {
    let mut sayac = 0;
    for grup in &g.gruplar {
        let cakismali = grup.iter().enumerate().any(|(i, &a)| {
            grup[i + 1..]
                .iter()
                .any(|&b| g.cakismalar.iter().any(|c| c.ilk == a && c.ikinci == b))
        });
        if cakismali {
            sayac += 1;
        }
    }
    sayac
}

/// Kullanılmayan yardımcıları susturmayı sağlayan referans.
#[allow(dead_code)]
fn _referans(_: &AtlananDosya, _: &KareKaydi, _: AlgisalHash, _: &dyn Fn(&Path)) {}
