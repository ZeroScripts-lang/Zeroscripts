# ZeroScript — Standard Library

42. String Standard Library

Length

string.length("Hello")
// 5

Contains

string.contains("Hello World", "World")
// true

Trim

string.trim("  Hello  ")
// "Hello"

Split

string.split("a,b,c", ",")
// ["a", "b", "c"]

Replace

string.replace("Hello World", "World", "ZeroScript")
// "Hello ZeroScript"

String operation tidak mengubah string asal.

---

44. Console Output

Standard output:

print("Hello")
print(10)
print(true)
print(null)

Error output:

print.err("Something went wrong")

"print.err()" hanya menulis ke error output. Ia tidak otomatis melempar runtime error.

---

45. Color

ZeroScript memiliki standard library "color".

color.red("Error")
color.green("Success")
color.yellow("Warning")
color.blue("Info")

Formatting dapat dikombinasikan:

color.bold(
    color.red("Error")
)

ZeroScript tidak bergantung pada Chalk atau Node.js untuk fitur ini.

---

48. File System

Standard library filesystem menggunakan namespace "file".

file.read("data.txt")
file.write("data.txt", "Hello")
file.exists("data.txt")
file.remove("data.txt")
file.mkdir("data")

Path relatif menggunakan working directory saat "zerosc" atau "zerovm" dijalankan.

Error filesystem dapat ditangani dengan "try/catch".

---

50. HTTP

Standard HTTP library:

http.get()
http.post()
http.put()
http.patch()
http.delete()

Contoh:

let res = await http.get("https://example.com")

Response:

res.status
res.headers
res.body

"res.body" selalu berupa "string".

ZeroScript tidak otomatis mengubah JSON response menjadi object.

50.1 HTTP ADDITIONAL CONCEPT 
- . HTTP — Request
API dasar tetap:
http.get(url)
http.post(url)
http.put(url)
http.patch(url)
http.delete(url)
Tapi biar bisa bikin REST API beneran, kita tambahkan options object sebagai argumen kedua:
let res = await http.post("https://example.com/users", {
    headers: {
        "Content-Type": "application/json"
    },
    body: json.stringify({
        nama: "Kael"
    })
})
Untuk GET juga:
let res = await http.get("https://example.com/data", {
    headers: {
        "Authorization": "Bearer token"
    }
})
Response
Tetap:
res.status
res.headers
res.body
Contoh:
if res.status == 200 {
    let data = json.parse(res.body)
    print(data)
}
body selalu string, jadi ZeroScript tidak akan otomatis mengubah JSON menjadi object.
Timeout
Kita tambahkan:
let res = await http.get("https://example.com", {
    timeout: 5000
})
timeout dalam milliseconds.
Kalau timeout atau network failure → runtime error → bisa ditangkap:
try {
    let res = await http.get("https://example.com", {
        timeout: 5000
    })
} catch e {
    print.err(e)
}
Status HTTP
Yang penting: HTTP 4xx/5xx bukan runtime error otomatis.
Misalnya:
let res = await http.get("https://example.com/not-found")

print(res.status) // 404
---

51. JSON

Parsing dilakukan secara eksplisit.

let data = json.parse(res.body)

Object/list menjadi JSON:

let body = json.stringify({
    nama: "Kael"
})

Tidak ada automatic JSON detection.

---

52. System

Standard system library:

system.args()
system.env()
system.os()
system.arch()

CLI arguments

Jika program dijalankan:

zerovm app.zs hello world

Maka:

system.args()

menghasilkan:

["hello", "world"]

Nama file program tidak termasuk.

Environment variable

system.env("PORT")

Jika tidak ada → "null".

OS

system.os()

Contoh:

"linux"

Architecture

system.arch()

Contoh:

"arm64"

---

58. String API — tambahan
string.startsWith(A, prefix)
string.endsWith(A, suffix)

string.lower(A)
string.upper(A)

string.indexOf(A, sub)
Contoh:
let A = "Hello World"

string.startsWith(A, "Hello") // true
string.endsWith(A, "World")   // true

string.lower(A) // "hello world"
string.upper(A) // "HELLO WORLD"

string.indexOf(A, "World") // 6
Aturan:
Semua fungsi menerima string.
startsWith() dan endsWith() → boolean.
lower() dan upper() → string baru, string asli tidak berubah.
indexOf() → index kemunculan pertama, mulai dari 0.
Kalau substring tidak ditemukan → -1.
Argumen dengan tipe salah → ERROR.
String tetap immutable.

----------- ------

59. HTTP Query Parameters

Kita tambahkan query pada options HTTP:

let res = await http.get("https://example.com/search", {
    query: {
        q: "zerocscript",
        page: 2
    }
})

ZeroScript akan menghasilkan URL:

https://example.com/search?q=zerocscript&page=2

Aturan:

query harus berupa object.

Key harus string.

Value boleh string, number, boolean, atau null.

Semua value di-URL-encode secara otomatis.

null direpresentasikan sebagai nilai kosong.

Query yang sudah ada di URL tetap dipertahankan.

query tidak mengubah URL asli secara langsung; hanya membentuk request URL.

Jika query bukan object → ERROR.


Contoh:

let res = await http.get(
    "https://api.example.com/users?sort=name",
    {
        query: {
            page: 2,
            active: true
        }
    }
)

Hasil request secara konsep:

https://api.example.com/users?sort=name&page=2&active=true

--- ----

60. HTTP Headers
Kita formalkan headers supaya request dan response punya aturan konsisten.
Request
let res = await http.get("https://example.com", {
    headers: {
        "Authorization": "Bearer token",
        "Accept": "application/json"
    }
})
Aturan:
headers harus berupa object.
Key harus string.
Value harus string.
Header dikirim sesuai key/value yang diberikan.
Tidak ada konversi otomatis untuk value.
Jika headers bukan object → ERROR.
Jika nilai header bukan string → ERROR.
Response
let res = await http.get("https://example.com")

print(res.headers)
res.headers berupa object dengan:
{
    "content-type": "application/json",
    "content-length": "1234"
}
Header response bersifat read-only terhadap response tersebut. Mengubah res.headers tidak mengubah response atau request yang sudah terjadi

---
