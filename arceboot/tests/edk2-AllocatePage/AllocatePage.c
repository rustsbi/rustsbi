#ifndef EFIAPI
#define EFIAPI
#endif

/* basic integer aliases */
typedef unsigned long long  u64;
typedef unsigned int        u32;
typedef unsigned short      u16;
typedef unsigned char       u8;

/* ---- Offsets (match the layout that produced SystemTable) ---- */
enum { OFF_ST_CONOUT        = 64 };  /* EfiSystemTable->conOut */
enum { OFF_ST_BOOTSERVICES  = 96 };  /* EfiSystemTable->bootServices */
enum { OFF_CONOUT_OUTPUTSTRING = 8 };/* EfiSimpleTextOutputProtocol->output_string */
enum { OFF_BS_ALLOCATEPAGES = 40 };  /* BootServices->AllocatePages */

/* AllocatePages() parameters */
enum {
  kAllocateAnyPages    = 0,
  kEfiBootServicesCode = 3
};


#define MAX_OUTPUT_CHARS 260

typedef u64 (*EfiTextString)(void* This, u16* String);
typedef u64 (*EfiAllocatePages)(u32 Type, u32 MemoryType, u64 Pages, u64* Memory);

/* 
 * Ensure instruction cache is synchronized with recently written code.
 * On RISC-V, `fence.i` flushes the instruction pipeline and makes sure
 * subsequent instruction fetches observe the updated memory contents.
 */
static inline void fence_i(void) { __asm__ __volatile__("fence.i" ::: "memory"); }

static void mem_copy(void* dst, const void* src, u64 n) {
  u8* d = (u8*)dst;
  const u8* s = (const u8*)src;
  while (n--) *d++ = *s++;
}

/* ASCII -> UTF-16 (Char16) and print via ConOut->OutputString */
static void put_ascii(void* SystemTable, const char* s) {
  if (!SystemTable || !s) return;

  void* con_out = *(void**)((u8*)SystemTable + OFF_ST_CONOUT);
  if (!con_out) return;

  EfiTextString OutputString = *(EfiTextString*)((u8*)con_out + OFF_CONOUT_OUTPUTSTRING);
  if (!OutputString) return;

  u16 buf[MAX_OUTPUT_CHARS];
  u32 i = 0;
  for (; s[i] && i < (MAX_OUTPUT_CHARS - 1); ++i) buf[i] = (u16)(u8)s[i];
  buf[i] = 0;

  OutputString(con_out, buf);
}

/* print hex64 with optional label, ends with CRLF */
static void put_hex64(void* SystemTable, const char* label, u64 v) {
  static const char hex[] = "0123456789ABCDEF";
  char tmp[2 + 16 + 2 + 1]; /* "0x" + 16 nybbles + "\r\n" + NUL */
  int p = 0;

  if (label) put_ascii(SystemTable, label);

  tmp[p++] = '0';
  tmp[p++] = 'x';
  for (int i = 15; i >= 0; --i)
    tmp[p++] = hex[(unsigned)((v >> (i * 4)) & 0xF)];
  tmp[p++] = '\r';
  tmp[p++] = '\n';
  tmp[p] = 0;

  put_ascii(SystemTable, tmp);
}

/* tiny payload: "ret" for RISC-V (jalr x0, x1, 0 -> 0x00008067) */
static const u8 payload_ret_only[4] __attribute__((section(".payload"))) = { 0x67, 0x80, 0x00, 0x00 };

typedef u64 (*payload_entry_t)(u64, u64, u64, u64, u64);

/* Entry: alloc exec pages, copy payload, fence.i, call, log, return */
u64 EFIAPI _ModuleEntryPoint(void* ImageHandle, void* SystemTable) {
  (void)ImageHandle;

  void* BootServices = *(void**)((u8*)SystemTable + OFF_ST_BOOTSERVICES);
  if (!BootServices) return 1;

  EfiAllocatePages AllocatePages = *(EfiAllocatePages*)((u8*)BootServices + OFF_BS_ALLOCATEPAGES);
  if (!AllocatePages) return 2;

  put_ascii(SystemTable, "[OK] C AllocatePages started\r\n");

  u64 payload_size = (u64)sizeof(payload_ret_only);
  if (payload_size == 0) {
    put_ascii(SystemTable, "[ERR] payload_size=0\r\n");
    return 5;
  }

  u64 pages = (payload_size + 0xFFFu) >> 12;
  if (pages == 0) pages = 1;

  u64 exec = 0;
  u64 st = AllocatePages(kAllocateAnyPages, kEfiBootServicesCode, pages, &exec);
  if (st != 0 || exec == 0) {
    put_hex64(SystemTable, "[ERR] AllocatePages st=", st);
    return st ? st : 6;
  }

  put_hex64(SystemTable, "[OK] exec_addr=", exec);
  put_hex64(SystemTable, "[OK] pages    =", pages);

  mem_copy((void*)(u64)exec, payload_ret_only, payload_size);
  fence_i();

  put_ascii(SystemTable, "[OK] calling payload...\r\n");

  payload_entry_t entry = (payload_entry_t)(void*)(u64)exec;
  u64 expected = 0xDEADBEEF12345678ull;
  u64 ret = entry(expected, 0, 0, 0, 0);

  put_hex64(SystemTable, "[OK] payload_ret=", ret);
  put_ascii(SystemTable, "[OK] done\r\n");

  return 0;
}
