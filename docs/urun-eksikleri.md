# İlk sürüm öncesi eksik listesi

Kullanıcının isteği (2026-11-11): "Ürünü büyük bir firmanın ürünüymüş gibi eleştirsin; ne eksik bulalım. Görsel konular bitince ürünleştirmeye geçelim."

Kaynaklar:

- İki bağımsız inceleme:
  - **Tasarım ve kullanım.** Sahte arka uçla bütün sayfalar gezildi: TR/EN, açık/koyu tema, 1360×900 ve 1040×700 pencere.
  - **Ürün hazırlığı.** Kod ve belgeler okundu.
- Bu oturumun kendi gözlemleri.

İşaretler:

- ✔ Kodda doğrulandı.
- **Şart:** ilk sürümden önce yapılmalı.
- **Olmalı:** ilk sürümde olması iyi olur.
- **Sonra:** ilk sürümden sonra.

Önerilen sıra:

1. Kimlik (A).
2. Güven (B).
3. Görsel ve içerik turu (C, D).
4. Veri ve tanılama (E).
5. Ürünleştirme (F) ve repo (G).

GitHub sayfası planı (H) ve kullanıcının kararları en sonda.

## A. Kimlik: isim, logo, uygulama kimliği

- **Yapıldı (2026-11-11): isim Keymotion, logo üç çizgili "K"** (tuvaldeki E konsepti; kullanıcının seçimi). Elenen adlar: Keypane, Tessera, Keyglow, Keywright, Keystride; KeyStage, Keylume, KeyLoom, KeyPulse ve Keyflow adlarında uygulama zaten var.
- **Yapıldı: uygulama kimliği** `dev.klavye.studio` → `dev.keymotion`. Kimliğe bağlı olanlar:
  - ayarların klasörü (`%APPDATA%`);
  - kurulumun kayıt defteri anahtarları;
  - güncelleyici;
  - bildirim kimliği.

  Eski klasörler ilk açılışta yeni ada taşınır (`migrate.rs`), WebView verisi de dahil.
- **Yapıldı:** İngilizce arayüzde yarım çevrilmiş ad ("Klavye Studio"), pencere başlığı ve ekran kartı örneği artık Keymotion.
- **Yapıldı:** Uygulama simgeleri (ICO 16–256 ve PNG'ler) yeni logodan üretildi. Logoda degrade ve parlama yok; 16 pikselde de okunuyor. Kurulum simgesi ve tepsi simgesi ürünleştirmede yapılacak.
- **Yapıldı (2026-11-12):** GitHub repo adı `keymotion` (eski adresler yönleniyor). Rust paketi `keymotion`, exe `keymotion.exe`, kütüphane `keymotion_lib`. Veri klasörleri kimliğe bağlı olduğu için taşıma gerekmedi.
- **Olmalı:** Bağlantı sayfasındaki "Aygıt: SONiX AK980 PRO" satırı çip üreticisinin adını gösteriyor. Model adı yeterli.

## B. Güven ve güvenlik

1. **Şart: fabrika animasyonu yedeği yalnız geliştiricinin bilgisayarında var.** Uygulama yedeği `%APPDATA%`'da arıyor; repoda yok ve olmamalı. Başka bir kullanıcının ilk yüklemesi ya da ilk canlı kartı fabrika animasyonunu geri dönüşsüz siler.
   - Yükleme onayı bunu söylüyor, ama canlı modu açarken bir uyarı yok.
   - Seçenekler:
     - (a) ilk yüklemeden önce açık bir onay;
     - (b) kullanıcının kendi indirdiği resmi sürücü paketinden yedeği çıkaran yönlendirmeli bir adım (bizde yapılan işlemin aynısı).
2. **Şart ✔: yarıda kalan yükleme.** Kütüphane eski görseli "klavyede" diye göstermeye devam ediyor (`lib.rs` `screen_upload`, `Some(_) if flash_written` dalı). Görsel sayfası "yarım" olarak işaretlenmeli ve bir sonraki açılışta yeniden yükleme önerilmeli.
3. **Şart: arka plandaki yüklemeler korunmuyor.** Canlı modun "kendi görseline dönüş" yüklemesi yaklaşık 5 dk sürüyor. Kapatma uyarısı yalnız elle yüklemeyi biliyor; Windows kapanırken de yazma kesilebilir.
   - Kapatma uyarısına eklenmeli.
   - Yazma sırasında `ShutdownBlockReasonCreate` kullanılmalı.
4. **Şart: başka yazılımlar aynı HID arayüzüne aynı anda konuşabilir.** Bunlar resmi sürücü, SignalRGB, OpenRGB, `kb` aracı ya da uygulamanın ikinci bir kopyası olabilir. Yazma sırasında komutlar karışabilir (ders 5).
   - Tek örnek (single instance) bir güvenlik düzeltmesidir.
   - Bilinen süreçler çalışırken yazma bir mesajla engellenmeli.
5. **Şart ✔: bozuk ya da daha yeni sürümlü ayar dosyası sessizce varsayılana dönüyor** (`settings.rs` `load`) ve bir sonraki kayıtta üzerine yazılıyor. `keymap.json` ve `macros.json` tek kayıt; klavyeden geri okunamıyor. Dosya `.bak` olarak saklanmalı ve kullanıcıya söylenmeli.
6. **Şart: ömür rakamları kesinmiş gibi yazılmış.** Örnekler: "yaklaşık 100.000 yükleme", "34 / 11 / 5,7 yıl" (`sc.wear.hint`, `sc.live.years.*`). Flash çipi bilinmiyor (DURUM §8); bu, yazım kılavuzunun "Doğru" ilkesine aykırı. Tahmin olarak yazılmalı ya da rakam kaldırılmalı.
7. **Olmalı: klavye yazılım sürümü** USB'den okunuyor (`bcdDevice`, 0x0106) ama gösterilmiyor. Doğrulanmamış bir sürümde yazmadan önce uyarı verilmeli.
8. **Olmalı: arayüzde olup klavyede denenmeyen atamalar.** Makroda "tekrar basana kadar" ve "basılı tuttukça"; farenin ileri tuşu, çift tık ve tekerlek. Kullanıcıyla denenmeli ya da gizlenmeli.
9. **Olmalı: kurtarma rehberi.** Klavye bozuk bir duruma düşerse ne yapılacağı. Fabrika ayarı kısayolu (Fn+Space) bu klavyede doğrulanmadı.
10. **Sonra:** İki klavye takılıysa yazma ilk bulunana gidiyor; ikisi birden takılıyken yazma reddedilmeli.
11. **Karar: kare sınırı 140 mı 135 mi?** 140, resmi sürücünün bu klavye için verdiği sınır. Bizde 135 kare denendi; 136–140 denenmedi. Kardeş klavyede bozulma 214 karede olmuş. Önerim: resmi sınır olduğu için 140 kalsın.

## C. İlk açılış, gezinme ve görünüm

1. **Şart: klavye yokken ilk açılış çıkmaz sokak.** Sorunlar:
   - Ayarlar dışındaki her sayfa "Klavye bekleniyor" diyor. Odak zamanlayıcısı, Sistem göstergeleri ve Hatırlatıcılar klavyesiz de çalışabiliyor.
   - Metin "Bluetooth ya da 2.4 GHz" diyor; oysa onlarla neredeyse hiçbir şey yapılamıyor.
   - Desteklenen model söylenmiyor.

   Gereken bir karşılama ekranı:
   - desteklenen model;
   - "USB kablosunu takın, arkadaki anahtarı ortaya alın" yönlendirmesi;
   - Bağlantı sayfasına bağlantı.

   Uygulama içi sayfalar önizleme hâlinde açılmalı.
2. **Şart ✔: pencere yaygın dizüstü ekranlarına sığmıyor.** En küçük boyut 1040×700. 1920×1080 ekranda %150 ölçekte kullanılabilir alan yaklaşık 1280×672. 1040×700'de de sorunlar var:
   - Ekran → Yazı kartında sağ sütun taşıyor ve yatay kaydırma çıkıyor;
   - tuş yazıları 6,4 piksele düşüyor.

   Öneriler:
   - dar pencerede kenar çubuğu simgelere küçülsün;
   - sütunlar alt alta insin;
   - en küçük yükseklik düşürülsün;
   - %125, %150 ve %175 ölçekte denensin.
3. **Şart: klavye çizimi rengi iyi göstermiyor.**
   - Boyanan tuşlar krem kalıyor, renk yalnız alttaki ince şeritte görünüyor. Boyanmayan tuşlar beyaz görünüyor, oysa klavyede sönük.
   - Uygulamada üç ayrı klavye çizimi var: Genel Bakış/Işıklar/Tuş atama, Odak ve Göstergeler.

   Tek çizici olmalı; tuşun üstü tuşun rengini almalı, sönük tuşlar koyu çizilmeli.
4. **Olmalı: kaydetme her düzenleyicide farklı.**
   - Efektlerde "Klavyeye uygula" düğmesi sayfanın altında kalıyor.
   - Tuş başına renkte "Klavyeye kaydet" ve ayrıca bir önizleme anahtarı var.
   - Tuş atama ve Makrolarda kaydetme yan kartta.
   - Tek bir değişiklik 4–5 yerde sayılıyor: kenar noktası, sekme rozeti, düğme rozeti, "1 kaydedilmedi" ve "1 tuş değişti".

   Her düzenleyicinin altında aynı eylem çubuğu olmalı: "N değişiklik · Geri al · Klavyeye kaydet".
5. **Olmalı: adlar ve yerler.**
   - Canlı özelliğin üç adı var: "Canlı ekran" (sekme), "Otomatik güncelleme" (başlık) ve "Canlı mod" (anahtar).
   - Kenar çubuğunda "Ekran", sayfa başlığında "Ekran stüdyosu" yazıyor.
   - Takvim Ekran → Canlı altında saklı; Hatırlatıcıların yanına taşınabilir.
   - "Ayarlar" hem bir sayfa hem birkaç sayfada bölüm başlığı.
   - Bağlantı durumu 4 yerde tekrarlanıyor.
6. **Olmalı: Genel Bakış'ın içeriği zayıf.** Önerilen ekler:
   - şu anki efekt ve parlaklık ayarı;
   - ekranda ne olduğu;
   - odak zamanlayıcısını hızlı başlatma;
   - klavye yazılım sürümü.

   USB'de pil kartı tek satıra inebilir. En belirgin düğme zaten kendiliğinden yapılan "Şimdi eşitle".
7. **Olmalı: okunurluk.**
   - Açık temada turuncu düğmedeki beyaz yazının kontrastı yaklaşık 3,0:1.
   - Yardımcı metin rengi (`--ink-3`) yaklaşık 3,3:1. AA için 4,5:1 gerekir.
   - Devre dışı "Klavyeye kaydet" düğmesi soluk ama etkin gibi duruyor.
   - Tuş başına renkte ekran okuyucu onaltılık rengi okuyor ("Q #ff8a3d").
8. **Olmalı: görsel tutarlılık.**
   - Kılavuza aykırı kart içinde kart: Araç kutusu, Odak, Göstergeler, Canlı kaynaklar, Hatırlatıcılar.
   - 21 efektin küçük resmi neredeyse aynı ("Sabit" gökkuşağı gösteriyor).
   - İki farklı renk paleti var.
   - Sayfa genişlikleri farklı.
   - Bluetooth pil göstergesi parlıyor.
   - Ekran sayfasındaki üç kaynak kartı farklı stilde.
   - Tema sırası "Sistem / Koyu / Açık"; Windows'ta "Açık / Koyu / Sistem".
9. **Olmalı: hareket.** Hareket kitaplığı Windows'un "animasyonları azalt" ayarına uymuyor (`MotionConfig reducedMotion="user"` eksik).

## D. İçerik

- **Şart ✔:** "Düzen %98" metninde `%` elle yazılmış. İngilizcede "Layout %98" çıkıyor; doğrusu "98% layout".
- **Şart:** Biçim hataları:
  - Makrolarda "0.00 s"; Türkçede "0,00 sn" olmalı.
  - Kalibrasyonda "Gama 2.20".
  - Kılavuzun yasakladığı "X: Y" ve "·" zincirleri: "Bugün: 0 · Toplam: 0", "Son eşitleme: … · 3 dk önce".
- **Şart:** Arayüzde iç ayrıntılar var: "Klavye belleği 0 / 2700 B", "16 parça", yazı tipi adları. Bazı yerlerde mühendislik notu gibi uzun paragraflar var: Tuş atama bilgi kutusu, Esc yanıp sönmesi, göstergelerdeki "%10" açıklaması.
- **Şart:** Arayüzde gelecek sözü verilmiş: "2.4 GHz alıcıyla ayar desteği sonraki bir sürümde gelecek." Bağlantı sayfasındaki bu not kaldırılmalı.
- **Şart:** 2.4G yönlendirmeleri çelişiyor. Işıklar sayfası yalnız "kabloyla bağlayın" diyor; anahtarın ortaya alınması gerektiğini söylemiyor. Ekran sayfası doğru.
- **Şart ✔:** "AJAZZ fabrika animasyonu" etiketi arka uçta Türkçe yazılmış (`screen_lib.rs`); İngilizce arayüzde çevrilmiyor.
- **Olmalı:**
  - "+ Sol tık" yerine "Sol tıklama".
  - Işıklar, Ekran stüdyosu ve Ayarlar sayfalarında başlık altı açıklama yok.
  - "Türkçe harfler desteklenir" İngilizce kullanıcıya bir şey söylemiyor.
  - "Bilinmiyor" GIF önizlemesi büyük siyah bir kutu.

## E. Veri, gizlilik ve tanılama

- **Şart ✔: kurulu sürüm günlük tutmuyor.** Günlük yalnız konsola gidiyor (`lib.rs` `tracing_subscriber::fmt`) ve kurulu sürümün konsolu yok.
  - Boyutu sınırlı, dönen bir günlük dosyası tutulmalı (`%LOCALAPPDATA%`).
  - Çökme olursa bir not yazılmalı.
- **Şart: "Tanılama bilgisini kopyala".** İçerik: uygulama sürümü, Windows sürümü, WebView2, model, klavye yazılımı, bağlantı ve son günlük satırları. `friendlyError` gerçek hatayı gizlediği için kullanıcının bildirebileceği başka bir şey yok.
- **Şart: gizlilik açıklaması** (`PRIVACY.md` ve Hakkında). Kapsaması gerekenler:
  - telemetri yok;
  - ağa yalnız takvim adresi ve güncelleyici için çıkılıyor;
  - yerelde okunanlar: çalan şarkı, işlemci/ekran kartı, mikrofon sessizliği, boşta kalma süresi;
  - dosyaların yeri.
- **Olmalı:** Dışa ve içe aktarma: ayarlar, tuş atamaları, makrolar ve ekran kütüphanesi tek bir dosyada. Ayrıca "Veri klasörünü aç". Işık durumu yalnız tarayıcı belleğinde (`lighting.ts`, localStorage); arka uca taşınmalı.
- **Olmalı:** Takvimin gizli adresi `calendar.json`'da düz metin ve Roaming klasöründe. Windows DPAPI ile şifrelenmeli ya da Local'e taşınmalı.
- **Olmalı:** Arayüzde bir hata yakalayıcı (ErrorBoundary) yok; bir çizim hatası boş pencere bırakır.
- **Olmalı:** Kaldırma: WebView klasörü (`EBWebView`) de silinmeli. Klavyenin atama, makro ve ekranı kaldırmadan sonra da koruduğu belgelenmeli.
- **Olmalı:** Uyku/uyanma, Uzak Masaüstü ve kullanıcı değiştirme denenmedi.

## F. Ürünleştirme: planlı işler için risk notları

- **Şart, karar:** Tepsi ve arka plan servisi. Plan, pencere kapanınca WebView'ı kapatmak. Ama canlı kartlar (şarkı, odak, takvim, hatırlatıcı) WebView'da çiziliyor (`liveCards.ts` → `live_submit`). WebView kapanırsa kartlar durur. Kartlar Rust'ta mı çizilecek, yoksa gizli bir pencere mi kalacak? Servis yazılmadan önce karar verilmeli.
- **Şart:** `tauri.conf.json`'da NSIS ayarları eksik:
  - yayıncı (bugün "klavye");
  - telif;
  - lisans sayfası;
  - TR/EN kurulum dili;
  - kullanıcı başına kurulum;
  - WebView2 kurulum biçimi.
- **Şart:** Sürüm hattı. Etiketle tetiklenen iş şunları yapmalı: Tauri CLI ile derleme, imzalama, güncelleyici bildirimi ve SHA-256, kurulu sürümün bir kez açılması.
  - Düz `cargo build --release` boş pencere açar, çünkü geliştirme sunucusunu arar.
  - İmza anahtarları onaylı bir ortamda tutulmalı. Otomatik birleştirme açık ve ikinci göz yok; tek bir kötü birleştirme herkese gider.
  - Güncelleyicinin gizli anahtarı çevrimdışı yedeklenmeli. Kaybolursa kurulu sürümler bir daha güncellenemez.
- **Olmalı:** Kod imzalama. EV sertifikası artık SmartScreen'i atlatmıyor. Açık kaynak için ücretsiz SignPath Foundation ya da Azure Artifact Signing değerlendirilmeli; Türkiye'deki bireysel geliştirici için uygunluğu kontrol edilmeli.
- **Olmalı:** Hatırlatmalar tepsi modunda Windows bildirimiyle de gelmeli; bugün klavye kablolu değilse yalnız ses var.
- **Olmalı:** Güç kullanımı.
  - Ayna, pencere küçültülünce de saniyede 4 okuma yapıyor.
  - HID taraması 2 sn'de, Bluetooth taraması 10 sn'de bir sürekli çalışıyor.
  - Pencere gizliyken durmalı ve pilde yavaşlamalı. Boşta işlemci ve bellek ölçülmeli.
- **Olmalı:** Sürüm numarası üç dosyada duruyor ve tutarlılığı denetlenmiyor. CHANGELOG yok.

## G. Repo ve hukuk

- **Şart: repo açık ama LICENSE yok.** Hukuken kimse kullanamaz. Rust dünyasında yaygın olan MIT/Apache-2.0 ikilisi; çatallar açık kalsın isteniyorsa GPL-3.0.
- **Şart:** Üçüncü taraf lisans bildirimleri: yaklaşık 450 crate, npm paketleri, hidapi ve lucide. Ekran kartlarında kullanılan üç OFL yazı tipi de var (Bricolage Grotesque, Instrument Sans, JetBrains Mono); OFL metninin dağıtılması gerekir. Bildirimler araçla üretilmeli (cargo-about ve bir JS aracı) ve Hakkında'da gösterilmeli.
- **Şart: "Resmî değildir" notu** README, kurulum ve Hakkında'da yer almalı:
  - uygulama AJAZZ ile bağlantılı değil;
  - markalar sahiplerine ait;
  - kalıcı belleğe yazma kullanıcının sorumluluğunda.

  İlk açılışta bir kez gösterilmeli.
- **Şart:** Fabrika GIF'i ve resmi sürücü dosyaları asla repoya ya da kuruluma girmemeli. Bugün doğru; CI'da bir koruma eklenmeli.
- **Olmalı:**
  - CI'a bozuk karakter ve BOM denetimi (ders 52).
  - ESLint `react-hooks`: dersler 20, 27 ve 33'ü yakalardı.
  - Sahte arka uçla e2e testleri. Sahte arka uçtaki kural kopyaları (`gauges.ts`, `pomodoroLights.ts`, `calendar.ts`, `reminders.ts`) Rust ile eşleşmeli.
  - Tedarik zinciri: cargo-deny/audit, Dependabot, sabit Rust sürümü.
- ~~**Sonra:** `?mock` ve `?pomofast` adres anahtarları kurulu sürümden çıkarılmalı.~~ Bitti: sahte arka uç yalnız geliştirme sunucusunda var.

## İyi olanlar (korunacak)

- **Bağlantı dürüstlüğü:** modlara göre kilitler, Bağlantı tablosu, sayı uydurmayan pil metni.
- **Yazma güvenliği:**
  - komut izin listesi;
  - bootloader engeli;
  - kare ve sayfa sınırları;
  - HID şekil denetimi;
  - yükleme onayında varsayılan düğme Vazgeç;
  - günlük yükleme sınırı.
- **Kaydedilmemiş değişiklik akışı:** taslaklar sayfa değişince kaybolmuyor; kapatırken "Kaydet ve çık" çıkıyor; silme yerinde onaylanıyor.
- **Ekran stüdyosunun adımları:** Kaynak → Düzenle → Önizle → Yükle, her adımda Vazgeç. Ayrıca kart şablonları ve yazım kılavuzu.
- **Küçük saldırı yüzeyi:** telemetri yok, sıkı CSP, yalnız `core:default` yetkisi.

## H. GitHub sayfası ve topluluk (sırası gelince)

Kullanıcının notu (2026-11-11): Repo açık kaynak olacağı için sayfalar hem İngilizce hem Türkçe olacak. İçerik:

- ekran görüntüleri: Türkçe ve İngilizce, açık ve koyu tema;
- kullanım, ayarlar ve bütün özellikler;
- uygulamanın denendiği ortam: Windows sürümü, klavye modeli ve yazılımı, bağlantı modları.

Büyük açık kaynak masaüstü uygulamalarının repoları incelenip örnek alınacak.

- **README:** İngilizce (`README.md`) ve Türkçe (`README.tr.md`), birbirine bağlantılı. Bölümleri:
  - kısa tanım ve "son sürümü indir" bağlantısı;
  - ekran görüntüleri;
  - özellikler ve bağlantı modlarına göre tablo (`destek-haritasi.md`'den);
  - desteklenen klavyeler;
  - kurulum, ilk açılış ve sık sorulanlar;
  - kurtarma ("bir şey ters giderse");
  - gizlilik özeti;
  - lisans ve resmî olmadığı notu.
- **Kullanım belgeleri:** sayfa sayfa anlatım (Işıklar, Tuş atama, Makrolar, Ekran, Odak, Göstergeler, Hatırlatıcılar, Profiller, Ayarlar), iki dilde, ekran görüntüleriyle. GitHub Pages ya da `docs/` altında.
- **Geri bildirim yolları:**
  - issue formları: hata bildirimi (tanılama çıktısı, uygulama sürümü, Windows, klavye yazılımı, bağlantı), özellik isteği, yeni klavye desteği isteği (USB kimliği, HID tanımlayıcıları);
  - Discussions: soru-cevap ve fikirler;
  - güvenlik açıkları için `SECURITY.md` ve özel kanal.
- **Topluluk dosyaları:** `CONTRIBUTING.md` (iki dilde; PR'lar `develop`'a, kontroller, donanım testi kuralları), davranış kuralları, PR şablonu, etiketler (`bug`, `enhancement`, `needs-hardware-test`, `model:…`), sürüm milestone'ları.
- **Sürümler:** Releases sayfasında imzalı kurulum, SHA-256, değişiklik notu (CHANGELOG'dan), iki dilde kısa özet.
- **Lisans ve haklar:** LICENSE-MIT ve LICENSE-APACHE, NOTICE (resmî olmadığı notu), üçüncü taraf lisansları, gizlilik (`PRIVACY.md` ve `docs/gizlilik.md`).
- **Dış katkılar için denetim:** DURUM P3'teki plan (zorunlu inceleme, CODEOWNERS, fork PR'larında gizli bilgilere erişim yok).

## I. Uygulama içinden sorun bildirme (kullanıcının fikri, 2026-11-12)

Kurulu sürümde bir hata olduğunda ya da bir özellik başka bir klavyede çalışmadığında, kullanıcı birkaç tıkla bize ulaşabilmeli. Gizlilik sözü korunur: uygulama kendiliğinden hiçbir şey göndermez, her bildirimi kullanıcı görüp kendisi yollar.

- **Yakalama (çoğu var):** panik notu (`crash-*.txt`), arayüz hataları için ErrorBoundary, başarısız işlemlerin günlüğe yazılması, "Tanılama bilgisini kopyala".
- **Gösterme:**
  - beklenmeyen hata: kısa ve sakin bir bildirim, "Sorunu bildir" düğmesiyle; aynı hata tekrar tekrar gösterilmez;
  - çökme: sonraki açılışta bir kez "Uygulama geçen sefer beklenmedik şekilde kapandı" ve "Bildir";
  - beklenen hatalar (kablo yok, başka program açık) bildirim istemez, bugünkü açıklayıcı metinleriyle kalır.
- **Bildirme penceresi:**
  - gönderilecek metni önce gösterir: sürüm, Windows, klavye modeli ve yazılımı, son günlük satırları (adresler ve kullanıcı adı zaten ayıklanıyor);
  - "GitHub'da bildir": hazır formu (`.github/ISSUE_TEMPLATE/bug.yml`) başlık ve özetle açar; uzun metin panoya kopyalanır, kullanıcı yapıştırır (adres uzunluğu sınırlı);
  - **Yalnızca GitHub** (kullanıcının kararı, 2026-11-12): e-posta seçeneği yok. GitHub hesabı olmayanlar için tanılama metni kopyalanabilir kalır.
- **Desteklenmeyen klavye:** AJAZZ/SONiX kimlikli ama listede olmayan bir klavye takılınca "Bu klavye henüz desteklenmiyor — destek iste". Bildirime VID/PID, ürün adı, arayüzler ve HID tanımlayıcıları eklenir (yalnızca okunur, klavyeye bir şey yazılmaz). Yeni model eklemek için en değerli bilgi bu.
- **GitHub tarafı (H ile birlikte):** issue formları (hata, klavye desteği, öneri), `SECURITY.md` (güvenlik açığı özel kanaldan), Discussions (soru-cevap), etiketler.
- **Yapılmayacak:** otomatik hata gönderme (Sentry vb.) ilk sürümde yok. İstenirse sonra, açıkça "izin ver" seçeneğiyle ve gizlilik belgesi güncellenerek.

## Kararlar (kullanıcı, 2026-11-11)

1. **İsim ve logo:** Keymotion; logo üç çizgili "K", biri turuncu yanıyor (A).
2. **Lisans:** MIT ve Apache-2.0 ikili lisans ("MIT OR Apache-2.0") (G).
3. **Fabrika animasyonu:** kullanıcı kararı bana bıraktı (B1). Karar ve gerekçe:
   - AJAZZ'ın animasyonu (kendi logosu) paketlenmez ve repoya konmaz: telif ve marka hakkı onlara ait.
   - Kullanıcı resmi sürücüyü kurduysa uygulama animasyonu kendi bilgisayarında bulup yedek alır.
   - Yedek yoksa ilk yüklemede ve canlı modu açarken açık bir onay alınır.
   - Resmi sürücünün fabrika animasyonunu geri yükleyebildiği söylenir.
   - İlk sürüm için Keymotion'ın kendi özgün açılış animasyonu da hazırlanacak; herkes geri dönebileceği bir görsele sahip olur.
4. **Kare sınırı:** 140, resmi sürücünün sınırı (B11).
5. **Profiller:** ilk sürümde olacak (E). Tasarımı önce önizlemeyle gösterilecek.
6. **Bütün "Şart" ve "Olmalı" maddeleri yapılacak.** Açılış ekranı dahil. Kullanıcı, alt ajanların kullanılabileceğini ve dış referanslara bakılabileceğini söyledi; amaç ürünün en üst seviyede olması.
7. **Açık kalan:** tepsi modunda canlı kartların nasıl çizileceği (Rust'ta mı, gizli pencerede mi) ürünleştirme başlarken kararlaştırılacak (F).

Ek kararlar (2026-11-12):
- Pencere kapatılınca uygulama tepside çalışmaya devam eder; Windows'la başlatma isteğe bağlı (varsayılan kapalı). Kalan işlerin sırası: `docs/yol-haritasi.md`.
- Uygulamanın ve GitHub sayfasının varsayılan dili İngilizce; ikisinde de Türkçe seçilebilir.
- Repo adı `keymotion` (yeniden adlandırıldı).
- GitHub Discussions açık (soru-cevap).
- Sorun bildirme yalnızca GitHub üzerinden.

## Uygulama durumu (2026-11-12, birleştirme sonrası)

Ayrıntı ve yardımcıların listesi: `DURUM.md` madde 38.

| Bölüm | Maddeler | Durum |
|---|---|---|
| A. Kimlik | İsim, logo, simgeler, uygulama kimliği, veri taşıma | **Bitti** (PR #53). Repo ve Rust paket adı kullanıcıya sorulacak. |
| B. Güven | B4, B5, B7, B10 | **Birleşti** (PR #57, açık). Gerçek uygulamada denendi: tek örnek, SignalRGB engeli, bozuk kayıt bildirimi, fw 1.06 ile kayıt |
| B. Güven | B1, B2, B3, B6 | **Birleşti** (PR #57). Denendi: yükleme sırasında kapatma. Denenmedi: kablo çekince "yarım kaldı", Windows'u kapatma |
| B. Güven | B8 (denenmemiş atamalar), B9 (kurtarma rehberi), B11 (140 kare: kalıyor) | B8 kullanıcıyla klavyede denenecek; B9 GitHub belgeleriyle birlikte |
| C. Görünüm | C1, C2, C5, C6 (açılış, dar pencere, adlar, Genel Bakış) | **Birleşti** (PR #57). Açılış ekranı yalnızca sahte cihazda görüldü |
| C. Görünüm | C3, C4, C7, C8, C9 (klavye çizimi, kaydetme çubuğu, okunurluk, tutarlılık, hareket) | **Birleşti** (PR #57) |
| D. İçerik | Bütün maddeler | **Birleşti** (PR #57) |
| E. Veri ve tanılama | Günlük, tanılama, ErrorBoundary, dışa/içe aktarma, ışık durumu, takvim şifresi, gizlilik belgesi | **Birleşti** (PR #57). Gerçek uygulamada denendi |
| E. Profiller | Profiller sayfası, Genel Bakış kartı | **Birleşti** (PR #57). Gerçek uygulamada dört yükleme denemesi geçti |
| F. Ürünleştirme | Tepsi, NSIS, imza, sürüm hattı, güncelleyici, güç | Sırası ve ayrıntısı `docs/yol-haritasi.md` (Aşama 1–6) |
| G. Repo ve hukuk | Lisans, NOTICE, üçüncü taraf lisansları, CI denetimleri, ESLint, Dependabot, cargo-deny, CHANGELOG | **Bitti** (PR #54; ESLint uyarıları temizlendi ve CI'ı engelliyor, `?mock` anahtarları kurulu sürümde yok) |
| H. GitHub sayfası | İki dilli README ve belgeler, ekran görüntüleri, issue formları, topluluk dosyaları | Ürünleştirmeden sonra. Kararlar verildi: varsayılan İngilizce (`README.md`) + Türkçe (`README.tr.md`), repo adı `keymotion` (yapıldı), Discussions açık (yapıldı) |
| I. Sorun bildirme | Uygulama içinden sorun bildirme, desteklenmeyen klavye bildirimi | Planlandı, ürünleştirmeyle birlikte (bölüm I) |
| Dil | Uygulamanın varsayılan dili | **İngilizce** (2026-11-12, yapıldı); Türkçe seçilebilir |
| Tepsi ve arka plan | Pencere kapanınca uygulamanın tepside çalışmaya devam etmesi | **Bitti** (PR #61, 2026-11-12): kapatınca gizlenir, tepsi menüsü (klavye, odak alt menüsü, hatırlatıcılar, göstergeler, profiller, çık), Ayarlar › Arka plan, Windows'la gizli başlama. Kullanıcı ve ben denedik; bildirimler Keymotion adı ve logosuyla. Ayrıntı: `docs/yol-haritasi.md` Aşama 2 |
| Keymotion açılış animasyonu | Fabrika yedeği olmayanlar için özgün görsel | **Bitti** (2026-11-12): kullanıcı A'yı (çizerek, yazılı) seçti; kütüphanede yerleşik "Keymotion animasyonu", yedek yok uyarısında öneri. Ayrıntı: `docs/yol-haritasi.md` Aşama 3 |
