Job 016 — Standard Library: HTTP

Objective

Implementasikan `http.*` sesuai §50, §50.1, §59, §60. Bergantung penuh pada
Job 014-015 (Async/Task, karena seluruh HTTP API menggunakan `await`) dan
Job 011 (pola integrasi builtin, `json.*` untuk contoh penggunaan response).

---

Required Reading — WAJIB

1. "AGENTS.md"
2. "StandardLibrary.md" §50, §50.1, §59, §60
3. Hasil Job 014-015 (Task/await)
4. Hasil Job 011 (json.parse/stringify, dipakai bersama http di contoh Spec)

---

Scope

### A. HTTP Methods (§50, §50.1)

- `http.get(url)`, `http.get(url, options)`
- `http.post(url, options)`, `http.put`, `http.patch`, `http.delete`
  (options sama untuk semua method)
- Setiap pemanggilan `http.*` menghasilkan Task (dipakai dengan `await`,
  mengikuti mekanisme Job 014).
- Options object dapat berisi: `headers`, `body`, `timeout`, `query`
  (lihat bagian C dan D di bawah).

### B. Response (§50, §50.1)

- `res.status` (number)
- `res.headers` (object, read-only terhadap response — lihat §60)
- `res.body` (SELALU string — tidak ada automatic JSON parsing, §50 & §51
  eksplisit menegaskan ini dua kali)
- HTTP status 4xx/5xx BUKAN runtime error otomatis — `res.status` tetap bisa
  dibaca normal (§50.1, bagian "Status HTTP").
- Timeout atau network failure → runtime error, catchable via try/catch.

### C. Query Parameters (§59)

- `options.query` — object, key harus string, value boleh
  string/number/boolean/null.
- ZeroScript membangun query string dan meng-append ke URL, URL-encode
  otomatis untuk semua value.
- `null` direpresentasikan sebagai nilai kosong dalam query string.
- Query yang sudah ada di URL asli tetap dipertahankan (di-append, bukan
  ditimpa).
- `options.query` bukan object → ERROR.

### D. Headers (§60)

- `options.headers` — object, key harus string, value HARUS string
  (berbeda dari query yang boleh multi-type).
- `options.headers` bukan object → ERROR.
- Header value bukan string → ERROR.
- `res.headers` adalah object hasil response, read-only terhadap response
  tersebut (mengubah `res.headers` tidak memengaruhi request/response yang
  sudah terjadi — tentukan bagaimana ini ditegakkan: apakah `res.headers`
  adalah copy independen, atau object heap normal yang kebetulan tidak
  pernah ditulis balik ke jaringan — jelaskan pilihan di final report).

### E. Timeout (§50.1)

- `options.timeout` dalam milidetik.
- Timeout atau network failure → runtime error, catchable try/catch (contoh
  persis §50.1).

---

Out of Scope

- Package/dependency system (Job 016-017 penomoran katalog Package berbeda
  dari job HTTP ini — jangan tertukar; job Package ada di 017-018)
- WebSocket, streaming response, atau protokol lain di luar yang disebutkan
  Spec (tidak ada di §50, jangan menambah)

---

Testing Requirements

Karena ini melibatkan network I/O nyata, gunakan mock/local test server atau
mekanisme test HTTP yang deterministik (jangan bergantung pada endpoint
publik eksternal yang tidak stabil untuk test suite).

1. `http.get` dasar mengembalikan `res.status`, `res.headers`, `res.body`
   sebagai string
2. `http.post`/`put`/`patch`/`delete` dengan body dan headers
3. `res.body` tidak pernah otomatis di-parse jadi object
4. HTTP 404/500 tidak melempar runtime error otomatis, `res.status` terbaca
5. query parameters ter-encode dengan benar di URL, termasuk value null
6. query bukan object → ERROR
7. headers terkirim sesuai key/value yang diberikan
8. headers bukan object → ERROR
9. header value bukan string → ERROR
10. timeout menghasilkan runtime error yang catchable
11. network failure (host tidak valid) menghasilkan runtime error yang
    catchable
12. kombinasi `json.stringify` sebagai body dan `json.parse` pada response
    (contoh persis §50.1)
13. regression seluruh test suite sebelumnya

---

Acceptance Criteria

- Seluruh contoh di §50, §50.1, §59, §60 berjalan sesuai Spec.
- Tidak ada automatic JSON conversion di layer HTTP.
- Tidak ada regresi.
- "CHECK.md" diperbarui untuk section HTTP.

---

Required Final Report

1. Files changed
2. Dependency HTTP client yang dipakai (jika ada) dan alasan pemilihannya
3. Bagaimana http.* terintegrasi dengan Task/await dari Job 014-015
4. Desain query encoding dan header validation
5. Tests added (termasuk setup mock server jika dipakai), total test count,
   hasil `cargo test`
6. Ambiguitas dan keputusan yang diambil
7. Known limitations
