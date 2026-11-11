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

Kullanıcının kararını bekleyenler en sonda.

## A. Kimlik: isim, logo, uygulama kimliği

- **Şart:** İsim ve logo seçimi. Adaylar ve logo tuvali sohbette sunuldu: Keypane (öneri), Tessera, Keyglow, Keywright. KeyStage, Keylume ve KeyLoom adlarında uygulama zaten var.
- **Şart ✔:** Uygulama kimliği `dev.klavye.studio` isimle birlikte ilk sürümden önce değişmeli. Kimliğe bağlı olanlar:
  - ayarların klasörü (`%APPDATA%`);
  - kurulumun kayıt defteri anahtarları;
  - güncelleyici;
  - bildirim kimliği.

  Sonradan değişirse kullanıcı verisi eski klasörde kalır. Geliştirme makinesindeki veriler bir kez taşınmalı.
- **Şart ✔:** İngilizce arayüzde ad yarım çevrilmiş ("Klavye Studio"; `app.name`, ekran kartı örneği). Pencere başlığı İngilizcede de "Klavye Stüdyo" kalıyor.
- **Şart:** Simge Windows 11'e uygun yeniden çizilmeli. Bugünkü simge degradeli ve parlamalı; 32 pikselde "K" okunmuyor. Gerekenler: ICO (16–256), kurulum simgesi ve tepsi simgesi.
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
- **Sonra:** `?mock` ve `?pomofast` adres anahtarları kurulu sürümden çıkarılmalı.

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

## Kullanıcının kararı gerekenler

1. **İsim ve logo** (A).
2. **Lisans:** MIT/Apache-2.0 mi, GPL-3.0 mı (G)?
3. **Fabrika animasyonu:** yalnız uyarı mı, yoksa kullanıcının resmi sürücü paketinden yedek alan bir adım mı (B1)?
4. **Kare sınırı:** 140 mı, 135 mi (B11)?
5. **Tepsi modunda canlı kartlar:** Rust'ta çizim mi, gizli pencere mi (F)?
6. **Profiller:** ilk sürümde mi, sonra mı? Dışa/içe aktarma ilk sürümde olmalı (E).
