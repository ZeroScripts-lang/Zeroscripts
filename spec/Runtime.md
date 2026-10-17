# ZeroScript — Runtime Configuration & Operations

55. Runtime Architecture

Pipeline utama ZeroScript:

.zs
 ↓
Lexer
 ↓
Parser
 ↓
AST
 ↓
Bytecode
 ↓
ZeroVM

Runtime direncanakan native dan tidak bergantung pada Node.js.

---

147. ZeroVM Runtime Context

ZeroVM menyediakan Runtime Context yang berisi keadaan dan fasilitas yang diperlukan selama program berjalan.

Runtime Context dapat mencakup:

global state
loaded modules
active tasks
standard library bindings
runtime configuration
resource limits
I/O interfaces

Aturan

- Runtime Context tersedia selama eksekusi program.
- Global state dan module state mengikuti aturan scope dan module ZeroScript.
- Active task menjadi bagian dari runtime state selama task masih hidup.
- Standard library memperoleh akses terhadap fasilitas runtime melalui context yang sesuai.
- Runtime configuration dapat menentukan batas resource atau fasilitas yang tersedia.
- Runtime Context tidak dapat diakses secara langsung sebagai object ZeroScript.
- Satu proses ZeroVM dapat memiliki lebih dari satu execution context jika implementasi runtime membutuhkannya.
- State antar execution context tidak boleh tercampur tanpa mekanisme yang memang ditentukan oleh runtime.
- Detail struktur Runtime Context merupakan detail internal ZeroVM.

Runtime Context menjadi penghubung antara execution engine ZeroVM dengan fasilitas yang dibutuhkan oleh program tanpa mengekspos struktur internal VM kepada ZeroScript.

148. ZeroVM I/O Boundary

ZeroVM menyediakan boundary untuk operasi input dan output yang digunakan oleh standard library dan runtime.

I/O dapat mencakup:

standard input
standard output
standard error
filesystem
network

Aturan

- Operasi I/O dilakukan melalui fasilitas runtime, bukan dengan mengakses memory internal ZeroVM secara langsung.
- Standard library dapat menggunakan I/O boundary untuk menyediakan API kepada ZeroScript.
- Operasi I/O dapat menghasilkan runtime error jika operasi gagal.
- Kegagalan internal VM tetap menjadi VM Error.
- Akses I/O dapat dibatasi oleh runtime environment.
- Program tidak memperoleh akses otomatis terhadap seluruh resource sistem.
- Detail implementasi I/O dapat berbeda antara platform selama behavior yang dijanjikan standard library tetap dipenuhi.
- Operasi I/O yang bersifat asynchronous dapat terintegrasi dengan sistem task ZeroScript.
- I/O tidak mengubah semantics dasar variable, scope, type, dan reference ZeroScript.

Dengan boundary ini, ZeroVM dapat berjalan pada berbagai environment tanpa membuat semantics bahasa bergantung langsung pada API sistem operasi tertentu.

149. ZeroVM Runtime Configuration

ZeroVM dapat menerima konfigurasi runtime yang menentukan environment tempat program dijalankan.

Konfigurasi dapat mencakup:

memory limit
execution limit
I/O permissions
filesystem access
network access
runtime features

Aturan

- Runtime configuration berlaku pada execution environment, bukan sebagai bagian dari syntax ZeroScript.
- Program tidak boleh mengubah batas keamanan atau resource runtime secara langsung tanpa mekanisme yang memang disediakan.
- Jika sebuah operasi dibatasi oleh configuration, operasi tersebut menghasilkan runtime error atau permission error sesuai fasilitas yang digunakan.
- Memory limit mengikuti aturan ZeroVM Memory Limits.
- Execution limit dapat digunakan untuk mencegah program berjalan tanpa batas.
- Permission terhadap filesystem dan network dapat diberikan atau dibatasi oleh environment.
- Configuration yang tidak valid sebelum program berjalan menghasilkan startup/runtime configuration error.
- Runtime configuration tidak mengubah semantics dasar bahasa.
- ZeroVM dapat menyediakan default configuration untuk environment tertentu.
- Detail format configuration file bukan bagian wajib dari syntax ZeroScript dan dapat ditentukan oleh implementasi runtime.

Runtime configuration memungkinkan ZeroVM digunakan baik sebagai runtime lokal maupun sebagai execution environment dengan resource dan permission yang lebih terbatas.

150. ZeroVM Execution Limits

ZeroVM dapat menerapkan batas terhadap resource yang digunakan oleh program selama execution.

Batas dapat mencakup:

execution time
memory usage
call stack depth
task count

Aturan

- Execution limit ditentukan oleh runtime environment.
- Program yang melewati execution limit dapat dihentikan dengan runtime/VM error sesuai jenis batas yang dilanggar.
- Memory limit mengikuti aturan ZeroVM Memory Limits.
- Call stack yang melebihi batas menghasilkan VM Error.
- Jumlah active task dapat dibatasi oleh runtime.
- Execution limit tidak mengubah semantics normal program selama batas belum tercapai.
- Runtime boleh tidak menerapkan batas tertentu jika environment tidak membutuhkannya.
- Program tidak boleh mengandalkan nilai batas tertentu karena batas dapat berbeda antar environment.

Execution limits digunakan sebagai perlindungan runtime dan resource management, bukan sebagai bagian dari bahasa ZeroScript.

151. ZeroVM Runtime Isolation

ZeroVM menjalankan program dalam execution environment yang terpisah dari internal state VM dan resource sistem yang tidak diberikan kepada program.

Aturan

- Program hanya dapat mengakses resource yang tersedia melalui standard library atau runtime interface.
- Program tidak dapat mengakses memory internal ZeroVM secara langsung.
- Program tidak dapat melakukan pointer arithmetic atau manipulasi raw memory.
- Akses filesystem, network, process, atau resource sistem lainnya dapat dibatasi oleh runtime configuration.
- Runtime error pada program tidak boleh merusak state internal ZeroVM atau execution context lain.
- Satu execution context tidak boleh membaca atau mengubah state execution context lain kecuali melalui mekanisme yang memang disediakan runtime.
- Heap, Call Stack, Value Stack, dan Runtime Context merupakan internal ZeroVM.
- Detail isolation dapat berbeda berdasarkan environment selama boundary tersebut tetap dipertahankan.

Runtime isolation menjadi dasar agar ZeroVM dapat digunakan pada berbagai environment tanpa memberikan akses sistem secara otomatis kepada program ZeroScript.

152. ZeroVM Resource Cleanup

ZeroVM bertanggung jawab terhadap pembersihan resource runtime yang sudah tidak digunakan.

Resource dapat mencakup:

heap data
execution frames
completed tasks
module runtime state
I/O handles
internal runtime resources

Aturan

- Heap data yang sudah tidak reachable dapat direclaim oleh garbage collector.
- Execution Frame dibuang setelah function selesai dan tidak lagi diperlukan.
- Task yang telah mencapai terminal state tidak dijalankan kembali.
- Resource yang dimiliki task harus dilepas ketika task tidak lagi membutuhkannya.
- Runtime harus menjaga agar resource yang masih digunakan tidak dilepas secara prematur.
- I/O resource dapat memiliki mekanisme cleanup khusus sesuai standard library atau runtime.
- Cleanup internal tidak boleh mengubah hasil observabel program secara tidak sesuai semantics ZeroScript.
- Jika cleanup resource gagal dan kegagalan tersebut dapat diamati oleh program, behavior-nya ditentukan oleh API yang bersangkutan.
- Detail mekanisme cleanup merupakan tanggung jawab implementasi ZeroVM.

Resource cleanup bertujuan menjaga runtime tetap stabil ketika program membuat banyak object, task, module, atau resource eksternal selama execution.

153. ZeroVM Program Termination

Program ZeroScript dapat berakhir melalui termination normal atau termination akibat error.

Termination normal terjadi ketika execution mencapai akhir entry point atau instruksi "HALT".

Termination akibat error dapat terjadi karena:

runtime error
uncaught error
VM Error
resource limit
runtime termination

Aturan

- Program yang mencapai akhir entry point tanpa error berakhir secara normal.
- "HALT" mengakhiri execution secara normal.
- Runtime error yang tidak ditangani menyebabkan program termination.
- VM Error dapat menghentikan execution karena merupakan kegagalan pada level VM.
- Resource limit yang tercapai dapat menyebabkan termination sesuai aturan runtime.
- Program tidak boleh terus mengeksekusi instruction setelah termination.
- Sebelum runtime benar-benar berhenti, ZeroVM dapat melakukan cleanup terhadap resource yang masih dimiliki execution context.
- Exit status atau mekanisme pelaporan termination dapat ditentukan oleh host runtime.
- Detail exit status bukan bagian dari semantics inti ZeroScript.

Termination menandai akhir lifecycle sebuah execution context ZeroVM.

154. ZeroScript Runtime Error Reporting

ZeroScript menyediakan informasi error ketika terjadi kegagalan saat program dijalankan.

Informasi error dapat mencakup:

error type
error message
source location
call stack

Aturan

- Runtime error memiliki informasi yang dapat digunakan oleh "catch".
- Error dapat memiliki source location yang menunjukkan lokasi operasi yang menyebabkan error jika informasi tersebut tersedia.
- Call stack dapat digunakan untuk membantu mengetahui jalur pemanggilan function ketika error terjadi.
- Error reporting tidak mengubah nilai atau state program secara sembarangan.
- Error yang berhasil ditangani oleh "try/catch" dapat dilanjutkan sesuai control flow program.
- Error yang tidak ditangani mengikuti aturan program termination.
- VM Error dibedakan dari runtime error ZeroScript.
- Detail format pesan error tidak menjadi bagian wajib dari semantics bahasa.
- Implementasi ZeroVM boleh menyediakan informasi tambahan untuk debugging selama tidak mengubah behavior program.

Contoh error yang dapat dilaporkan:

TypeError
IndexError
KeyError
DivisionError
ArgumentError
CancellationError

Nama kategori error tersebut merupakan konsep runtime dan dapat disesuaikan selama jenis kegagalan dapat dibedakan secara konsisten.

155. ZeroScript Debug Information

ZeroVM dapat menyediakan informasi tambahan untuk membantu debugging program ZeroScript.

Informasi debug dapat mencakup:

source file
line number
column number
function name
module name
call stack

Aturan

- Informasi debug berasal dari metadata yang dapat disimpan oleh compiler pada bytecode.
- Debug information tidak mengubah semantics program.
- Bytecode tetap dapat dijalankan tanpa debug information jika runtime mengizinkannya.
- Source location dapat digunakan untuk menghubungkan instruction bytecode dengan lokasi pada source ".zs".
- Call stack dapat menampilkan function yang sedang aktif ketika error terjadi.
- Compiler dapat menyediakan mode build dengan debug information dan mode tanpa informasi tersebut.
- ZeroVM tidak boleh bergantung pada source code asli hanya untuk menjalankan bytecode jika bytecode tidak membutuhkannya.
- Format internal debug metadata merupakan detail bytecode dan dapat berubah pada versi berikutnya.

Debug information ditujukan untuk membantu developer memahami execution tanpa menjadi bagian dari behavior inti bahasa ZeroScript.

156. ZeroScript Core Specification Completion

Section ini menandai selesainya rancangan core specification ZeroScript dan ZeroVM untuk tahap desain awal.

Core specification mencakup:

language syntax and semantics
variables and scope
types and operators
control flow
functions and closures
collections
strings and objects
error handling
modules
packages and dependencies
asynchronous tasks
concurrency
ZeroVM architecture
runtime values
memory management
garbage collection
bytecode
execution model
standard library boundary
runtime configuration
resource limits
isolation
debug information

Aturan

- Implementasi ZeroScript harus mengikuti semantics yang telah ditentukan oleh specification.
- ZeroVM menjadi runtime utama untuk menjalankan bytecode ZeroScript.
- Compiler bertanggung jawab menerjemahkan source ".zs" menjadi bytecode yang valid.
- Detail implementasi internal dapat berbeda selama behavior yang terlihat oleh programmer tetap sesuai specification.
- Specification dapat direvisi apabila implementasi menemukan masalah desain, ambiguity, atau kebutuhan baru.
- Perubahan terhadap semantics inti harus dicatat sebagai perubahan specification, bukan dilakukan secara diam-diam pada implementasi.
- Fitur baru dapat ditambahkan melalui section atau extension baru tanpa harus mengubah core behavior yang sudah stabil.
- Standard library dapat berkembang secara terpisah selama tetap mengikuti type system, error model, dan runtime boundary ZeroScript.
- Format bytecode dan implementasi ZeroVM dapat berkembang berdasarkan kebutuhan implementasi.

Dengan selesainya section ini, ZeroScript memiliki fondasi desain yang cukup untuk memasuki tahap implementation, testing, dan validation.

Specification ini tetap merupakan living specification dan dapat diperbarui berdasarkan hasil implementasi nyata.
