# Sürüm notları

Keymotion'daki önemli değişiklikler burada listelenir. İngilizcesi: [CHANGELOG.md](CHANGELOG.md).

1.0.0'dan itibaren her sürüm GitHub'da ve Microsoft Store'da aynıdır; ikisi
için tek bir değişiklik listesi vardır. Windows, Store'dan kurulan kopyanın
sürümünü sonuna her zaman 0 olan dördüncü bir sayıyla gösterir (1.0.0, 1.0.0.0
olur). Bir sürüm önce GitHub'da çıkar; Store kopyası, Microsoft onu
denetleyince arkasından gelir, genellikle bir gün içinde. Numarası da notları
da aynıdır. Deneme sürümleri ("-rc") yalnızca GitHub'dadır. Yalnızca birini
ilgilendiren bir değişiklik bunu belirtir.

## [Yayımlanmamış]

### Eklenenler

- **AJAZZ'ın 3.01 ("769") klavye yazılımındaki AK980 PRO, deneme aşamasında:**
  Keymotion artık USB kablosunda bu yazılımın farklı komut takımını
  kullanıyor: ışık efektleri ve tuş başına renk, canlı ışıklar, iki katmanda
  tuş atama, makrolar, ekrana görsel yükleme, saat ve klavyenin kabloda da
  bildirdiği pil düzeyi. Destek yeni ve hâlâ deneniyor. Bu yüzden **Bağlantı**
  sayfası ve Genel bakış onu **Deneme aşamasında** diye işaretliyor; nasıl
  çalıştığını aşağıdaki deneme rehberiyle bize anlatabilirsiniz. Keymotion her
  oturumda ilk yazmadan önce, denenmediği her klavye yazılımında olduğu gibi
  sorar (**Klavye yazılımı denenmedi**). **Devre dışı**, **Çift tıklama** ve
  **Basılı tuttukça** makro kipi bu yazılımda sunulmaz.
- **Deneme rehberi (3.01 klavye yazılımı):** 3.01 klavye yazılımı kullanan ve
  yardım etmek isteyenler için dokuz adımlık, yaklaşık on dakikalık, isteğe
  bağlı bir deneme: hazırlık, bağlantı, ışık efekti, canlı ışık, tuş başına
  renk, tuş atama, makrolar, ekran (isteğe bağlı) ve ilk yedeği geri yükleme.
  Genel bakış sizi sayfayı kapatmadan bir kez davet eder (**Denemeye başla**;
  bir sonraki bağlanışta bir kez daha soran **Şimdi değil**; **Bir daha
  gösterme**); **Bağlantı** sayfasında rehber, ne kadar ilerlediğinizle
  birlikte her zaman durur. Rehber sayfaların yanında açık kalır: **Oraya
  git** adımın gerektirdiği sayfayı açar, siz de **Çalıştı**, **Çalışmadı** ya
  da **Atladım** diye cevaplar, isterseniz bir not yazarsınız. Keymotion'ın
  kendisinin bilebildiği yerlerde (yedek, rehber açıkken kaydedilip klavyeden
  geri okunan bir ayar) adım işaretlenir; başka hiçbir zaman. Cevaplarınız bu
  bilgisayarda kalır ve o klavye yazılımına aittir: başka bir klavye
  yazılımında rehber baştan başlar. Keymotion'ın yalnızca okuduğu bir klavyede
  rehber, kaydetme isteyen adımların çalışamayacağını söyler ve **Sorun
  bildir**'i sunar. Sonunda rehber gönderilecek metni olduğu gibi gösterir
  (cevaplarınız ve notlarınız, uygulama ve Windows sürümü, Keymotion'ın klavye
  hakkında okudukları; makrolarınızın içeriği hiçbir zaman yok); **Kopyala ve
  GitHub'da aç** metni panoya koyar ve formu başlığı ve modeli yazılı olarak
  açar; rehber orada hangi cevabı seçeceğinizi söyler. Hiçbir şey
  kendiliğinden gönderilmez ve rehber klavyeye hiçbir şey yazmaz: kaydetmeyi
  her zamanki gibi sayfalarda siz yaparsınız. 1.06 klavye yazılımında hiç
  görünmez.
- **Klavye yedekleri (3.01 klavye yazılımı):** Keymotion klavyeye ilk
  bağlandığında, bir şey yazabilmeden önce klavyenin ayarlarının yedeğini
  alır (iki katmandaki tuş atamaları, ışık efekti, tuş başına renkler,
  makrolar); her geri yüklemeden önce bir yedek daha alır. **Bağlantı**
  sayfası yedekleri listeler. **Geri yükle**, kablodaki klavyenin yedeğini
  adım adım geri yükler: neler geri gelir, yedek ışığın rengini bilmiyorsa
  o renk, sonra neyi geri yüklediği ve hangi tuşları olduğu gibi bıraktığı.
  Ekrana bir yükleme sürerken geri yükleme başlamaz; başka bir sayfaya
  geçtiğinizde sürer ve nasıl bittiğini bir bildirim söyler.
  Yedek alınamazsa ya da klavyenin bildirdikleri beklendiği gibi görünmezse
  Keymotion klavyeye hiçbir şey yazmaz; yazacak her sayfa nedenini söyler.
- **Denetlenen yazmalar (3.01 klavye yazılımı):** Keymotion kaydettiği her
  ayarı (tuş atama, ışık, makrolar) geri okur. Klavye açıldıktan ya da
  sıfırlandıktan sonra bir süre yazılanları yok saydığı için Keymotion ilk
  yazmadan önce bekler. Tuş başına renkleri olmayan bir klavye, örneğin bir
  klavye yazılımı güncellemesinden (ya da AJAZZ'ın web aracıyla yapılan bir
  sıfırlamadan) sonra, olağandır ve Keymotion'ın yazmasını engellemez.
  Yarıda kalan bir kaydetmeyi, örneğin kablo çıktığında, aynı şeyin bir
  sonraki kaydı düzeltir.
- **3.01 klavye yazılımında tuş atama ve makrolar:** bu yazılım tuş
  atamasını ve makrolarını bildirir; bu yüzden **Tuş atama** ve **Makrolar**
  sayfaları, geri yüklemeden sonra da, klavyede olanı gösterir. Başka bir
  programda (örneğin AJAZZ'ınkinde) ayarlanan ve Keymotion'ın gösteremediği
  tuşlar ile makrolar ve Fn katmanındaki klavyenin kendi işlevleri öyle
  işaretlenir; kaydetmek ya da profil yüklemek onları olduğu gibi bırakır.
  Tuşlar ya da makrolar, Keymotion onları okuduktan sonra başka bir
  programda değiştiyse kaydetme hiçbir şey yazmaz ve sayfa klavyeyi yeniden
  gösterir. Tekrarlanan bir makro, Keymotion klavyeye canlı ışıklar
  dışında bir şey gönderdiğinde (örneğin bir kaydetmede) durur; tuşuna
  yeniden basın.
- **3.01 klavye yazılımında canlı ışıklar:** canlı ışıklar (müzik modu,
  sistem göstergeleri, odak zamanlayıcısı, hatırlatıcılar, takvim, oyun
  oturumu, yapay zekâ ajanları, uygulamaya göre profiller, **Klavyede
  önizle** ve kalibrasyonun test desenleri), eski klavye yazılımında olduğu
  gibi, klavyenin ışığı ne olursa olsun tuşlarda görünür. Hiçbiri klavyeye
  kaydedilmez: canlı ışıklar durunca klavyenin kendi ışığı yaklaşık 5
  saniye sonra kendiliğinden geri gelir. Keymotion klavyeye bağlandıktan
  hemen sonra ya da klavye sıfırlandıktan sonra canlı ışıklar birkaç saniye
  bekler; sayfalar bunu söyler.
- **Saat sayfası (3.01 klavye yazılımı):** yükleme klavyenin kendi saat
  sayfasının yerini alabilir; saat sayfası o zaman ancak klavyenin tam
  sıfırlamasıyla (Fn+Boşluk) geri gelir. Keymotion ilk yüklemeden önce,
  canlı ekranın ilk kartı da dahil, size sorar. Bu yazılımda süren bir
  yükleme durdurulabilir (**Yüklemeyi durdur**); Keymotion o zaman klavyeye
  başka bir şey göndermeden önce birkaç saniye bekler.
- **Başka bir program (3.01 klavye yazılımı):** klavye Keymotion'ın
  sormadığı cevaplar gönderirse Keymotion başka bir programın klavyeyle
  konuşuyor gibi göründüğünü söyler (tarayıcıda AJAZZ'ın klavye aracı açık
  mı?). Hiçbir şey engellenmez.
- **Sorun bildirimleri (3.01 klavye yazılımı):** klavyenin bildirdikleri
  beklendiği gibi görünmediğinde bildirim nedenini birkaç satırda söyler ve
  beklendiği gibi görünmeyen bölümü gösterir; çoğu zaman tek bildirim yeter.
  Keymotion bu okumayı, isteyebiliriz diye veri klasöründe de tutar
  (`keyboard-reads`, son beşi, makrolarınız olmadan); geri yükleme için
  hiçbir zaman önerilmez. Bildirimler GitHub için yeterince kısa kalır; kısaltılan
  her yer bunu belirtir.

### Değişenler

- Keymotion'ın henüz tanımadığı bir AJAZZ klavye yazılımındaki AK980 PRO,
  karşılama sayfasında yine kendi kartını alır; kartın metni artık böyle her
  yazılım için yazıldı ve **Destek iste** düğmesi duruyor.

## [1.2.1] - 2026-11-23

Keymotion 1.2.1, 1.2.0'ı Microsoft Store'a getiriyor. GitHub'dan kurulan
kopya için hiçbir şey değişmiyor.

### Düzeltilenler

- **Microsoft Store:** Store paketi `keymotion-cli`'ı artık Keymotion'ın
  kendi komutu olarak, Store'un kabul ettiği biçimde getiriyor; böylece
  1.2.0'ın yenilikleri Store kopyasına da geliyor. GitHub'dan kurulan
  kopya için hiçbir şey değişmiyor.

## [1.2.0] - 2026-11-23

Keymotion'ın görünüşü artık Windows 11 biçiminde ve yeni araçları var: müzik
modu, League of Legends ışıklarıyla oyun oturumu, uygulamaya göre profil ve
Claude Code, Codex gibi yapay zekâ kodlama ajanları için ışıklar. Uygulama
artık Rusça ve Basitleştirilmiş Çince olarak da var. Klavyenizde AJAZZ'ın
yeni 769 klavye yazılımı varsa Keymotion artık bunu açıkça söylüyor ve
klavyeye hiçbir şey göndermiyor; desteğin sonraki bir güncellemede gelmesi
planlanıyor.

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
- **Takvim:** La Paz / Mazatlán ya da Bişkek saat dilimindeki bir Outlook
  takviminin etkinlikleri yıl boyunca bir saat kaymış görünüyordu; yaz saati
  uygulanan birkaç saat diliminde (Indiana, Küba, Haiti, Aleut Adaları,
  Chatham Adaları, Lord Howe Adası ve birkaç yer daha) yazın bir saat
  kayıyordu. Keymotion artık Outlook'un kullandığı bütün saat dilimi
  adlarını tanıyor.
- **Bildirim alanı menüsü:** **Tuşlarda sistem göstergeleri**'ni kendi
  sayfasında açıp kapattığınızda menü bunu hemen gösteriyor. Önceden menü
  10 dakikaya kadar eski durumu gösterebiliyordu; o arada tıkladığınızda da
  gösterdiğinin tersini yapıyordu.
- **Genel bakış:** **Fırlatma** ya da **Halkalar** gibi yalnızca bastığınız
  tuşları yakan bir efektte, hiçbir tuşa basılmadığında klavyenin altındaki
  satır **Klavyenin ışığı kapalı** diyordu. Artık **Klavyenizin tuşları
  yazdıkça yanar** diyor.
- **Açılış animasyonu:** Windows 11'de artık uygulamayla aynı yarı saydam
  pencere arka planında oynuyor; uygulama göründüğünde renkler değişmiyor.
- **Işıklar:** **Tuş başına renk** sekmesinde **Klavyede önizle** açıkken
  pencereyi kapattığınızda, Keymotion saatin yanında çalışırken önizleme
  tuşlarda kalıyor; odak zamanlayıcısını, hatırlatıcıları, müzik modunu ve
  ajan ışıklarını örtüyordu. Önizleme artık pencere kapalıyken bekliyor,
  pencereyi açınca geri geliyor.
- **Ekran:** "…" ile biten bir şarkı, toplantı ya da hatırlatıcı adı
  artık en küçük boyutta çizilmiyor.
- **Kurulum:** Keymotion'ı GitHub'dan yüklü kopyanın üstüne yeniden
  kurduğunuzda **Bilgisayar açılınca Keymotion'ı başlat** artık kapanmıyor.

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

[Yayımlanmamış]: https://github.com/cebersaban44-rgb/keymotion/compare/v1.2.1...develop
[1.2.1]: https://github.com/cebersaban44-rgb/keymotion/releases/tag/v1.2.1
[1.2.0]: https://github.com/cebersaban44-rgb/keymotion/releases/tag/v1.2.0
[1.1.0]: https://github.com/cebersaban44-rgb/keymotion/releases/tag/v1.1.0
[1.0.0]: https://github.com/cebersaban44-rgb/keymotion/releases/tag/v1.0.0
[0.1.0]: https://github.com/cebersaban44-rgb/keymotion/releases/tag/v0.1.0
