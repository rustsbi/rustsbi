#include <Uefi.h>
#include <Library/UefiLib.h>

EFI_STATUS EFIAPI _ModuleEntryPoint(
    IN EFI_HANDLE ImageHandle,
    IN EFI_SYSTEM_TABLE *SystemTable
) {
    Print(L"Hello, World!");
    return EFI_SUCCESS;
}
