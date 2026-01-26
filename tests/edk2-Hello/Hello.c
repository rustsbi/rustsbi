#include <Uefi.h>
#include <Library/UefiLib.h>

//
// Use the standard EDK2 application entry point so that library constructors run
// (e.g. UefiLib / UefiBootServicesTableLib which initializes gST/gBS used by Print()).
//
EFI_STATUS EFIAPI UefiMain(
    IN EFI_HANDLE ImageHandle,
    IN EFI_SYSTEM_TABLE *SystemTable
) {
    Print(L"Hello, World!");
    return EFI_SUCCESS;
}
