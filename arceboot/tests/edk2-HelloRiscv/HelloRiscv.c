#include <Uefi.h>
#include <stdint.h>

typedef struct EfiTableHeader {
    uint64_t  signature;
    uint32_t  revision;
    uint32_t  headerSize;
    uint32_t  crc32;
    uint32_t  reserved;
} EfiTableHeader;

struct EfiSimpleTextOutputProtocol;
typedef uint64_t (*EfiTextString)(struct EfiSimpleTextOutputProtocol* this, int16_t* string);
typedef struct EfiSimpleTextOutputProtocol {
    uint64_t      reset;
    EfiTextString output_string;
    uint64_t      test_string;
    uint64_t      query_mode;
    uint64_t      set_mode;
    uint64_t      set_attribute;
    uint64_t      clear_screen;
    uint64_t      set_cursor_position;
    uint64_t      enable_cursor;
    uint64_t      mode;
} EfiSimpleTextOutputProtocol;

typedef struct EfiSystemTable {
    EfiTableHeader               hdr;
    int16_t*                     firmwareVendor;
    uint32_t                     firmwareRevision;
    void*                        consoleInHandle;
    uint64_t                     conIn;
    void*                        consoleOutHandle;
    EfiSimpleTextOutputProtocol* conOut;
    void*                        standardErrorHandle;
    uint64_t                     stdErr;
    uint64_t                     runtimeServices;
    uint64_t                     bootServices;
    uint64_t                     numberOfTableEntries;
    uint64_t                     configurationTable;
} EfiSystemTable;

EFI_STATUS EFIAPI _ModuleEntryPoint(
    void *imageHandle, EfiSystemTable* systemTable
) {
    
    systemTable->conOut->output_string(systemTable->conOut, (int16_t *)L"Hello, World!");

    return EFI_SUCCESS;
}