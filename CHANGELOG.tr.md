# Sürüm notları

Keymotion'daki önemli değişiklikler burada listelenir. İngilizcesi: [CHANGELOG.md](CHANGELOG.md).

1.0.0'dan itibaren her sürüm GitHub'da ve Microsoft Store'da aynıdır; ikisi
için tek bir değişiklik listesi vardır. Windows, Store'dan kurulan kopyanın
sürümünü sonuna her zaman 0 olan dördüncü bir sayıyla gösterir (1.0.0, 1.0.0.0
olur). Bir sürüm ikisinde aynı gün çıkar: GitHub sürümü, Microsoft Store
kopyasını denetleyene kadar bekler. Yalnızca acil bir düzeltme önce GitHub'da
çıkabilir ve bunu belirtir. Deneme sürümleri ("-rc") yalnızca GitHub'dadır.
Yalnızca birini ilgilendiren bir değişiklik bunu belirtir.

## [Yayımlanmamış]

### Eklenenler

- **Müzik modu:** Araçlar › **Müzik modu** tuş ışıklarını bilgisayarınızda
  çalan sese göre hareket ettirir (USB kablosuyla): **Çubuklar** (solda
  bastan sağda tize sütunlar alt sıradan yukarı dolar, tepede bir işaret bir
  an kalır), **Ritim** (bütün klavye her vuruşta parlar, güçlü vuruşta
  ortadan bir halka yayılır), **Düzey** (ortadan iki yana açılan bir ses
  yüksekliği şeridi) ve **Dalga** (her bas vuruşu tuşlar boyunca bir dalga
  gönderir). Renkler **Yeşil-sarı-kırmızı**, **Gökkuşağı**, **Tek renk** ya da
  **İki renk**; Çubuklar ve Dalga için **Ortadan**; %50 ile %200 arasında
  hassasiyet; diğer tuşlar karanlık ya da soluk. Ses durduktan birkaç saniye
  sonra klavyenin kendi ışığı geri gelir (ya da seçiminize göre bütün tuşlar
  soluk ya da karanlık olur). Odak zamanlayıcısı çalışırken ve oyun öndeyken
  (**Oyun oynarken dur**, başta açık) ışıklar bekler. Keymotion yalnızca
  hoparlörünüze ya da kulaklığınıza giden sesin o anki yüksekliğini,
  Windows'un kendi ses sistemiyle okur: Ses kaydedilmez, mikrofon
  kullanılmaz. Bildirim alanı menüsünde ve yedeklerde de var.
- **Oyun oturumu:** Araçlar › **Oyun oturumu** oynadığınızı fark eder.
  Oyunlar başlatıcılarınızın listelerinden (Steam, Epic Games, GOG, Xbox,
  Riot Games, Xbox Game Bar) ve sizin sözünüzden gelir (**Oyun ekle**,
  **Oyun değil**, **Geri al**); birkaç dakika ekranı kaplayan bir program için
  "Bir oyun mu?" diye sorulur. Bir oyun 15 saniye
  önde kalınca oturum başlar ve oyun açık kaldıkça, Alt+Tab'da da sürer.
  Canlı ekran açıksa başlarken klavyenin ekranına bir kart, 5 dakika ya da
  daha uzun bir oturum bitince bir özet (süre, ekran kartının yükü ve
  sıcaklığı, işlemcinin ortalama ve en yüksek yükü, bugünün toplamı) gider;
  oyun sürerken ekrana yeni bir şey gelmez. Oyunun tuşları (WASD, MOBA, 1–6
  ya da sizin seçtikleriniz) ya da bir profilin tuş tuş ışığı, klavyeye bir
  şey kaydedilmeden yanabilir; ışıklar öndeki pencereyi izler, Alt+Tab ile
  başka işe geçince bekler. Oyun listesi her programın simgesini ve aldığı
  profili gösterir, son oynanana ya da en çok oynanana göre sıralanır; Steam
  resmi olmayan bir oyunun başlangıç kartında simgesi görünür. Oyun sırasında
  gelen bir toplantı ya da hatırlatıcı kartı oyundan sonra kalkar. Geçmiş bu
  bilgisayarda kalır. "Kendiliğinden güncelle" oyunun bitmesini bekler.
- **League of Legends ışıkları:** maç sırasında, oyunun bilgisayarınızda
  sunduğu veriden: canınız bütün klavyede, azaldıkça seçtiğiniz yönde
  çekilerek (odak zamanlayıcısındaki gibi), ya da 1–0'da; mana F1–F12'de;
  ölünce koyu kırmızı, yeniden doğmaya yaklaştıkça geri gelen ışık (1–0'da
  geri sayılan kalan süre); zafer, yenilgi, seviye atlama, öldürme, as,
  ejderha, Baron ve Vadi'nin Alameti için seçtiğiniz renkte seçtiğiniz animasyon
  (dalga, parlama, nabız, kıvılcım, halka, yukarıdan sönme). Sayfadaki çizim
  bunları maçın kendi koduyla önizler, "Klavyede dene" klavyede gösterir.
  Oyun listesinde League of Legends'ın altında (Özel ışıklar), maçtaki
  sırasıyla gruplanmış olarak ayarlanır; kapatmadıkça ejderha, Vadi'nin Alameti
  ve Baron yalnız takımınız aldığında yanar. Özet kartı sonucu, K/D/A'yı ve
  minyon sayısını gösterir. Rakip ya da hedef sayaçları yok.
- **Uygulamaya göre profil:** Profiller › **Uygulamaya göre** bir programı
  ya da bütün oyunları bir profile bağlar. Varsayılan olarak yalnız tuş
  başına renk tasarımının ışığı değişir, klavyeye bir şey kaydedilmez; bir
  kural tuş atamasını ve makroları da kaydedebilir, günde en çok 20 kez
  (sonuncusu geri dönüşe ayrılır; başka bir klavye modeli için yapılmış
  profil atlanır ve sayılmaz). Oyunun tuş ataması ve makroları oyun boyunca kalır; oyun bitince klavye
  önceki haline döner (ya da seçtiğiniz profile geçer ya da olduğu gibi
  kalır), bu arada elle değiştirmediyseniz. Kurallar program güncellendikten
  sonra da tutar (Microsoft Store uygulamaları, Discord, Roblox). Simge
  menüsünden duraklatılabilir.
- **Oyun sürerken klavyeye hiçbir şey gönderilmesin:** oyun boyunca Keymotion
  klavyeye ışık, kart ya da profil göndermez, klavye kendi ışığında kalır;
  hile önleyicisi bundan rahatsız olabilecek oyunlar için. Kapatmadıkça
  toplantı ve hatırlatıcı uyarıları yine gelir; sessiz mod tek bir oyun için
  de açılabilir.
- **Kullanım koşulları:** sitede bir Kullanım koşulları sayfası var.
- **Rusça ve Basitleştirilmiş Çince:** uygulama, kullanım kılavuzu, kurulum
  ve README artık Rusça (Русский) ve Basitleştirilmiş Çince (简体中文) de
  var. Windows'un dili bu dördünden biriyse Keymotion o dilde başlar;
  **Ayarlar** › **Dil** dili hemen değiştirir. Çeviriler yapay zekâ
  yardımıyla yapıldı ve ana dili bu diller olan kişilerin incelemesini
  bekliyor: düzeltmeler memnuniyetle karşılanır. Çince arayüzde klavyenin
  adı, Çin'de satıldığı ad olan 黑爵 AK980 V2（带屏版）.
- **Yapay zekâ kodlama ajanları:**
  `keymotion-cli`, Claude Code'un, Codex'in ve betiklerin tuşları yakmasını
  sağlar: biten bir iş, sizi bekleyen bir soru, bir hata, bir ilerleme
  çubuğu ya da bir ajanın tuşlardan, renklerden ve efektlerden kurduğu her
  şey. `keymotion-cli connect claude-code` ya da `connect codex`, o
  uygulamanın ayarlarına birkaç kanca ve kısa bir yetenek ekler (önce neyin
  değişeceğini gösterir, bir kopya saklar, başka hiçbir şeye dokunmaz);
  ardından klavye ajan bitirince, onayınızı beklerken (siz cevap verene ya
  da bir tuşa dokunana kadar; Claude Code'un otomatik kipinde olduğu gibi
  ajan kendisi karar verdiğinde değil), hata verince ya da kullanım sınırına
  takılınca (yavaş mor) kendiliğinden yanar; iki ya da daha çok oturum
  varken bekleyenin numarası rakam sırasında yanar. Keymotion çalışmıyorsa
  böyle bir olay onu başlatır; siz kapattıysanız ya da ajan erişimini
  kapattıysanız başlatmaz. Sizin ya da ajanın yazdığı hiçbir şey Keymotion'a
  ulaşmaz: yalnız ne olduğu. `disconnect` hepsini geri alır. Ajan erişimi
  siz açana kadar kapalı kalır; GitHub kopyasında açmak `keymotion-cli`'ı
  yeni açılan terminaller için PATH'inize de ekler.
  Araçlar › **Yapay zekâ ajanları** onu iki adımda açar (Claude Code'a ve
  Codex'e ne ekleneceği, sonra ne yapıldığı; ilkinde Vazgeç), her
  uygulamanın durumunu gösterir; beş sinyalin rengini, efektini ve süresini
  **Klavyede dene** ile, ajanların başka neler yapabileceğini, ek
  davranışları, ajanların bu ayarları değiştirip değiştiremeyeceğini, oturum
  numaralarını ve **Geri al**'lı son işlemleri ayarlar; yapılmayan bir
  isteğin nedenini de söyler. Gizlilik politikası ve güvenlik politikası,
  ajanların neyi ilettiğini ve neyi hiçbir zaman değiştiremeyeceğini
  anlatır. `keymotion-cli` Keymotion'la birlikte gelir: GitHub'daki kurulum
  onu kendine ait bir `bin` klasörüne koyar ve kaldırma, Keymotion'ın
  Claude Code ile Codex bağlantılarını ve klasörünü PATH'inizden siler;
  Microsoft Store kopyası onu her terminalde yazılabilen bir komut olarak
  getirir. Üç şey daha, yalnız kendi izniyle ve her biri başta kapalı:
  ajanlar odak zamanlayıcısını başlatıp duraklatabilir ya da durdurabilir
  (ayarlarınızı değiştirmeden kendi seçtikleri uzunlukta tek bir blok da),
  bir profilin ışığını bir süre gösterebilir ya da klavyeye yazabilir
  (yalnız ışığını; bütün ajanlar birlikte herhangi bir 24 saatte en çok 10
  kez) ve ekrana yazılı ya da görselli bir kart koyabilir (bütün ajanlar
  birlikte saatte en çok 4, sizin ayarladığınız kadar; sonra kendi
  görseliniz geri gelir). Ekran kartlarına izin verdiyseniz 10 dakika ya da
  daha uzun süren bir kodlama işi bitince ekranda "Bitti · 14 dk" yazar.
  Ajanlar tuşlara harf harf bir kelime de yazabilir (`keymotion-cli spell
  "DONE"`) ve kendi küçük animasyonlarını oynatabilir (`scene`); bunları bir
  adla saklayıp yeniden oynatabilirler (`pattern`). Hepsi klavyenin çalışma
  belleğinde, saniyede en çok üç yanıp sönmeyle.

### Değişenler

- **Windows 11 biçiminde yeni görünüş:** sayfalar Windows Ayarlar'daki
  gibi soldaki bir bölmede; bölmenin başında bağlı klavye (adı ve nasıl
  bağlandığı ya da "Klavye bekleniyor"), listeyi daraltan bir arama kutusu,
  en altta Ayarlar. Ayarlar tam genişlikte satırlar, anahtar, liste ya da
  düğme sağda; düğmeler, anahtarlar, kaydırıcılar, sekmeler, pencereler ve
  kaydet çubuğu Windows'unkiler gibi, turuncu yalnızca vurgu rengi. Klavye
  çizimi ve ışıkları olduğu gibi kalıyor. **Genel bakış** klavyenin cam
  şeridiyle başlıyor (ekranı, camdaki resim ve son ayarlanan parlaklığıyla
  döner düğme), sonra klavye, sonra Keymotion'ın yaptığı her iş için bir
  satır; hatırlatıcılar ve sistem göstergeleri orada açılıp kapanabiliyor.
  **Işıklar** efektleri bir galeride, ayarlarını satırlarda gösteriyor;
  **Yön** bir liste. **Ekran** GIF sayfasını büyük, saat sayfasını yanında
  gösteriyor; kütüphane tek komut çubuklu bir resim ızgarası: bir görseli
  seçmek için tıklayın, sonra **Yükle**, **Işıkları eşleştir**, **Yeniden
  adlandır** ya da **Sil**. **Görsel ya da video**, **Yazı kartı** ve
  **Slayt gösterisi** aynı çubukta; bir dosya kütüphanenin üzerine
  bırakılabiliyor. Ayarlar, pencereler ve öteki sayfalar da aynı görünüşte.
  Listeler Windows 11'deki gibi açılıyor; seçili olan kısa turuncu bir
  çizgiyle belli.
- Yazmadan önce resmî yazılımı ya da SignalRGB'yi bulmak için kullanılan
  çalışan programlar listesi artık hiçbir programı açmıyor.
- **Tanıma:** AJAZZ AK980 PRO olarak yalnızca USB adında, denenen klavyenin
  bildirdiği gibi "AK980 PRO" geçen klavye sayılıyor. Aynı USB kimliğini
  taşıyan başka bir AK980, örneğin ekransız olanı, artık **Destek iste**'yi
  görüyor.
- **AJAZZ'ın yeni klavye yazılımı:** AJAZZ'ın farklı bir komut takımı
  kullanan 769 (3.01) yazılımındaki bir AK980 PRO artık yalnızca henüz
  desteklenmeyen bir klavye olarak görünmüyor. Karşılama sayfası, soldaki
  bölme ve saatin yanındaki simgenin menüsü, klavyede AJAZZ'ın yeni yazılımı
  olduğunu, Keymotion'ın ona hiçbir şey göndermediğini ve desteğin sonraki
  bir güncellemede gelmesinin planlandığını söyler; SSS'ye bir bağlantı da
  vardır. **Destek iste** yerinde kalır. Henüz desteklenmeyen başka bir
  klavye de artık soldaki bölmede "Klavye bekleniyor" yerine kendi adıyla
  görünür; karşılama sayfası da böyle bir klavyenin takılı olduğunu söyler.
- **Denenmemiş klavye yazılımı:** Keymotion'la denenmemiş bir yazılım
  sürümünde klavyenin saati de onayınızı bekliyor; hem bağlanınca eşitleme
  hem **Şimdi eşitle**. Keymotion kaydetmeden önceki soruyu soruyor,
  verdiğiniz onay uygulama kapanana kadar geçerli. Canlı ışıklar sormadan
  çalışmayı sürdürüyor.

### Düzeltilenler

- **Takvim ve hatırlatıcılar:** saatlerin geri alındığı gece 03:00'e
  (Avrupa'nın çoğunda; başka yerlerde değişikliğin bittiği saat) kurulu bir
  hatırlatıcı bir saat erken çalıyordu; o saatteki, kendi saat dilimi
  olmayan bir takvim etkinliği de bir saat erken görünüyordu.
- **Takvim:** gece yarısı tekrarlanan bir etkinlik, saat değişikliğinin
  gece yarısını atladığı ya da iki kez yaşattığı günde (Şili, Küba ve birkaç
  yer daha) artık kaybolmuyor. Ayın sondan beşinci pazartesisi gibi bir
  tekrarın yalnızca ilk tarihi değil, bütün tarihleri görünüyor. Kuralında
  "ilk pazartesi" yazan haftalık bir tekrar artık her gün değil, yalnızca
  pazartesileri geliyor.
- **2.4 GHz alıcı:** yalnızca alıcı takılıyken Keymotion artık klavyenizin
  çalıştığını söylemiyor. Bağlantı sayfası ve Genel bakış, alıcının takılı
  olduğunu ve Keymotion'ın klavyenin açık olup olmadığını henüz
  anlayamadığını söylüyor; yanındaki nokta gri. Klavye Bluetooth'la
  bağlıyken alıcı takılı kalırsa uygulama ve bildirim alanı menüsü alıcıyı
  değil, Bluetooth'u gösteriyor.
- AK980 PRO'nun USB kimliğini ve adını taşıyan ama içi farklı yapılmış bir
  klavye, her işlemin başarısız olduğu bir AK980 PRO gibi görünüyordu. Artık
  **Destek iste**'yi görüyor ve ona hiçbir şey gönderilmiyor.
- **Genel bakış:** **Fırlatma** ya da **Halkalar** gibi yalnızca bastığınız
  tuşları yakan bir efektte, hiçbir tuşa basılmadığında klavyenin altındaki
  satır **Klavyenin ışığı kapalı** diyordu. Artık **Klavyenizin tuşları
  yazdıkça yanar** diyor.
- **Açılış animasyonu:** Windows 11'de artık uygulamayla aynı yarı saydam
  pencere arka planında oynuyor; uygulama göründüğünde renkler değişmiyor.

## [1.1.0] - 2026-11-16

Keymotion artık yeni sürüm çıkınca haber veriyor ve isterseniz kendiliğinden
kuruyor. Microsoft Store'da da var.

### Eklenenler

- **Microsoft Store:** Keymotion 16 Kasım 2026'dan beri Microsoft Store'da.
  Oradaki kopyayı Microsoft imzalar; Windows onu uyarı göstermeden kurar.
  Sürüm sayfası, README ve site, GitHub indirme düğmesinin yanında
  Microsoft'un "Microsoft'tan alın" rozetini gösterir.
- **Güncelleme bildirimleri:** Yeni sürüm çıkınca, her sürüm için bir kez,
  **Şimdi güncelle** ve **Sonra** düğmeli bir Windows bildirimi bunu söyler.
  Saatin yanındaki Keymotion simgesinin menüsü **Güncelleme var** satırıyla
  başlar ve simgede küçük bir nokta görünür; pencerenin sol altındaki kart
  ve **Ayarlar**'ın yanındaki nokta da söyler. Önceden yeni sürüm yalnızca
  pencerenin içinde görünüyordu; Keymotion saatin yanında çalışırken fark
  edilmiyordu.
- **Kendiliğinden güncelleme:** Ayarlar › Hakkında › **Güncellemeler**'de
  tek anahtarın yerine üç seçenek var. **Kendiliğinden güncelle** yeni
  sürümü indirir ve bilgisayar kullanılmadığı bir anda kurar; klavyeye
  yazarken, odak zamanlayıcısı çalışırken ya da klavyeye kaydedilmemiş
  değişiklik varken asla. **Haber ver** (varsayılan) size söyler, ne zaman
  kuracağınızı siz seçersiniz; **Kapalı** kendiliğinden hiç bakmaz. Eski
  anahtar kapalıysa seçim **Kapalı** olur.
- Güncellemeden sonra bir Windows bildirimi hangi sürümün kurulduğunu
  **Neler yeni** düğmesiyle söyler; Keymotion da eskisi gibi açılır:
  penceresiyle ya da saatin yanında.

### Değişenler

- **İki kopya birden kuruluysa:** Bir bilgisayarda hem GitHub'dan hem
  Microsoft Store'dan kurulan Keymotion varsa GitHub kopyası artık açılınca
  Store kopyasını açar; bilgisayar açılınca başlama da Store kopyasına geçer.
  Önceden ikisi de bilgisayar açılınca başlamaya çalışır, önce davranan
  çalışırdı. Store kopyası Ayarlar › Hakkında bölümünde bir "GitHub kopyası"
  satırı gösterir; satırdaki düğme Windows'un Yüklü uygulamalar sayfasını
  açar ve GitHub kopyası oradan kaldırılabilir. İki kopya ayarları paylaştığı
  için ayarlar kalır.
- Keymotion açıkken bilgisayar açılışında ikinci kez başlatılırsa pencereyi
  artık öne getirmez.

### Düzeltilenler

- **Microsoft Store:** Store yeni sürümü kurarken Windows çalışan
  Keymotion'ı kapatır. Keymotion artık ardından kendiliğinden, saatin
  yanında yeniden açılır. Önceden bir sonraki oturum açılışına kadar kapalı
  kalıyordu.

## [1.0.0] - 2026-11-15

Keymotion 1.0. Uygulama 0.1.0 ile aynı, Microsoft Store'a hazır. Microsoft
Store 0 ile başlayan sürüm numarası kabul etmediği için bu sürüm 1.0.0.

### Eklenenler

- **Microsoft Store sürümü:** Aynı uygulama, Microsoft Store için
  paketlendi. Store'daki sürümü Microsoft imzalar; Windows onu uyarı
  göstermeden kurar. Store'dan kurulan Keymotion'ı Store günceller (kendi
  güncelleme denetimi kapalı kalır), bilgisayar açılınca Windows'un başlangıç
  ayarlarıyla başlar ve bildirimlerinde Store uygulamasının adı ve logosu
  görünür. GitHub'daki sürüm eskisi gibi çalışır.

## [0.1.0] - 2026-11-12

İlk sürüm. Windows'ta AJAZZ AK980 PRO'yu destekler.

### Eklenenler

- **Bağlantı:** Klavye USB kablo, 2.4 GHz alıcı ve Bluetooth bağlantısında
  tanınır; takıldığı ya da çıkarıldığı fark edilir. Bağlantı sayfası her modda
  neler yapılabildiğini gösterir; ayarlar için USB kablo gerekir. Bluetooth
  bağlantısında pil düzeyi gösterilir.
- **Genel bakış:** Sayfa genişliğince, ölçülerek çizilmiş 97 tuşlu düzen ve her
  tuşun üzerindeki harfler; ardından klavye ekranında şu an görünen görsel ve
  her araç için bir kutucuk: ışık, odak zamanlayıcısı (buradan
  başlatılabilir), sıradaki hatırlatma, sıradaki toplantı, canlı çubuklarıyla
  sistem göstergeleri ve profil. Ayrıca bağlantı modu, pil ve klavye saati;
  saat, klavye bağlanınca kendiliğinden ayarlanır.
- **Işıklar:** Renk, parlaklık, hız ve yön ayarıyla 19 hazır efekt; klavyenin
  renklerinin canlı yansıması; fırça ve seçim araçlarıyla tuş başına renk,
  hazır desenler, canlı önizleme ve klavyeye kaydetme.
- **Renk kalibrasyonu:** Klavyede gösterilen test desenleriyle gama ve beyaz
  dengesi ayarı.
- **Tuş atama:** Temel katman ve Fn katmanı. Bir tuşa başka bir tuş, kısayol,
  medya tuşu, fare düğmesi ya da kaydırma atanabilir veya tuş devre dışı
  bırakılabilir; bir katman fabrika düzenine sıfırlanabilir.
- **Makrolar:** Tuşlara basılarak kaydedilir, adım adım düzenlenir (bekleme
  süreleri, sıra, fare tıklamaları) ve bir oynatma biçimiyle tuşlara atanır;
  klavye belleğinde ne kadar yer kaldığı görünür.
- **Ekran stüdyosu:** 240×135 ekran için görseller, GIF'ler ve videolar
  (kırpma, süre), yazı kartları ve slayt gösterileri; yüklemeden önce
  önizleme, kütüphane ve fabrika animasyonunu geri yükleme. "Işıkları
  eşleştir", görseldeki renklerden tuş renkleri tasarlar.
- **Canlı ekran:** Kendiliğinden güncellenen kartlar (çalan şarkı, odak
  zamanlayıcısının aşamaları, yaklaşan toplantılar); günlük yükleme sınırı,
  ekranda neyin görüneceğini belirleyen açık bir sıra ve ardından kendi
  görselinize dönüş. Odak zamanlayıcısı ve şarkı, son yükleme hakkını
  hatırlatmalara ve toplantılara bırakır. Sınırı bekleyen bir kart, klavyede
  o an görünenden ayrı gösterilir ve beklediği belirtilir; "Gönderme" o kartı
  sıradan çıkarır.
- **Takvim:** Google, Outlook ve iCloud takvimleri paylaşım adresleriyle
  eklenir (yalnızca okunur); toplantıdan önce ekranda bir kart çıkar,
  başlamasına 1 dk kala bir tuş yanıp söner.
- **Hatırlatıcılar:** Bir kez, her gün ya da çalışma saatlerinde aralıklarla;
  tuşlarda bir renk dalgası, isteğe bağlı olarak ekranda bir kart ve ses.
  "Dene" kartı da gösterir; sayfa, bir kartın neden gösterilmediğini belirtir
  (yükleme sınırı, canlı ekranın kapalı olması, USB kablonun takılı olmaması,
  toplantı kartı).
- **Sistem göstergeleri:** İşlemci, ekran kartı ve bellek yükü tuş
  sıralarında çubuk olarak, ses düzeyi işlev tuşlarında, mikrofonun sessize
  alındığı seçtiğiniz bir tuşta gösterilir.
- **Odak zamanlayıcısı:** Odak ve mola bloklarını istediğiniz uzunlukta ve
  istediğiniz sırayla dizerek kendi sıranızı kurarsınız (blokların yerini
  sürükleyerek değiştirebilirsiniz); sıra bir kez, birkaç kez ya da siz
  durdurana kadar çalışır. Başlangıçta "25 / 5 klasik" seçilidir; eski
  ayarlar, karşılık geldikleri sıraya dönüştürülür. Bütün klavye bloğun
  rengiyle dolar ve süre azaldıkça boşalır; her blok için ekranda bir kart
  çıkar.
- **Kurulum:** Windows için İngilizce ve Türkçe kurulum programı; uygulamayı
  yalnızca sizin hesabınıza kurar, Keymotion simgesini ve sade bir karşılama
  metnini gösterir, yönetici izni gerektirmez. Kaldırırken "Ayarlarımı ve
  kayıtlarımı da sil" işaretlenmezse ayarlarınız korunur; kaldırma işlemi
  Windows'la başlatma kaydını ve bildirimler için kaydedilen adı da siler.
- **Sorun bildirme:** Ayarlar › Hakkında › Geri bildirim, beklenmeyen bir
  hatadan sonra çıkan göze batmayan uyarı ve uygulama çöktükten sonra bir kez
  çıkan uyarı, sorun bildirme penceresini açar; pencere gönderilecek bilgileri
  önce gösterir. "GitHub'da bildir" bilgileri kopyalar ve projenin sorun
  formunu varsayılan tarayıcıda açar; uygulama kendiliğinden hiçbir şey
  göndermez. Henüz desteklenmeyen bir SONiX ya da AJAZZ klavye için "Destek
  iste" çıkar; isteğe klavyenin Windows'tan okunan USB kimlikleri ve HID
  tanımları eklenir (klavyeye hiçbir şey yazılmaz).
- **Keymotion animasyonu:** Klavye ekranı için Keymotion'ın kendi animasyonu,
  kütüphanede hazır: logodaki "K" çizgi çizgi çizilir, ardından Keymotion
  yazısı görünür (37 kare, yüklemesi yaklaşık 1,5 dk). Fabrika animasyonunun
  yedeği yoksa önerilir.
- **Saatin yanındaki simge:** Pencereyi kapattığınızda Keymotion saatin
  yanında çalışmaya devam eder; böylece odak zamanlayıcısı, hatırlatıcılar,
  takvim kartları, canlı ekran ve tuşlardaki göstergeler durmaz. Simgenin
  menüsü klavyeyi ve pilini gösterir, odak zamanlayıcısını yönetir (aşama ve
  kalan dakika, duraklatma, sonraki aşamaya geçme, durdurma), sıradaki
  hatırlatmayı gösterir, hatırlatıcıları ve tuşlardaki göstergeleri açıp
  kapatır. Menüden bir profilin yükleme onayı da açılır ve uygulamadan
  çıkılır. Bu davranış Ayarlar › Kapatma ve açılış bölümünden kapatılabilir;
  aynı yerden Keymotion'ın Windows açılınca penceresiz başlaması da
  seçilebilir. Uygulama
  açıldıktan sonra pencere ilk kez kapatıldığında, Keymotion'ın hâlâ
  çalıştığını bildiren bir uyarı görünür.
- **Güncellemeler:** Ayarlar › Hakkında bölümünden GitHub'da yeni sürüm olup
  olmadığı denetlenir; uygulama da açıldığında günde bir kez kendiliğinden
  bakar (bu kapatılabilir). Yeni sürüm bulununca sayfanın üstünde bir şerit
  görünür; "Kur ve yeniden başlat"ı seçmediğiniz sürece hiçbir şey kurulmaz.
  İndirilen dosya uygulamanın kendi anahtarıyla doğrulanır; kurulum, klavyeye
  süren bir yazma işleminin bitmesini bekler.
- **Açılış animasyonu:** Pencere açılırken logodaki "K" çizgi çizgi çizilir ve
  Keymotion adı açılır, ardından logo soldaki menüde kendi yerine geçer
  (uygulama hazırlanırken yaklaşık bir saniye). Keymotion Windows açılınca
  penceresiz başladığında ya da saatin yanındaki simgeden yeniden açıldığında
  oynatılmaz; bir tıklama ya da tuşa basma animasyonu bitirir; Windows'ta
  animasyon efektleri kapalıysa yalnızca solarak kaybolur.
- Sayfadan ayrılırken ya da uygulama kapatılırken kaydedilmemiş değişiklikler
  için koruma.
- Türkçe ve İngilizce arayüz, açık ve koyu tema.
- Proje: MIT/Apache-2.0 lisansı, NOTICE (resmî olmayan proje, kullanım riski
  size aittir), üçüncü taraf lisans bildirimleri ve bozuk karakterler, üretici
  dosyaları, sürümler ve bağımlılıklar için otomatik denetimler.
- **Belgeler:** cebersaban44-rgb.github.io/keymotion adresinde Türkçe ve
  İngilizce kullanım kılavuzu: uygulamanın her sayfası, bir şey ters giderse ne
  yapılacağı, desteklenen ve sırada olabilecek klavyeler, geliştiriciler için
  sayfalar. Ayarlar › Hakkında'dan açılır.

[Yayımlanmamış]: https://github.com/cebersaban44-rgb/keymotion/compare/v1.1.0...develop
[1.1.0]: https://github.com/cebersaban44-rgb/keymotion/releases/tag/v1.1.0
[1.0.0]: https://github.com/cebersaban44-rgb/keymotion/releases/tag/v1.0.0
[0.1.0]: https://github.com/cebersaban44-rgb/keymotion/releases/tag/v0.1.0
