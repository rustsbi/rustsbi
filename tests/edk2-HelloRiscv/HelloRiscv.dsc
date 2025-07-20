[Defines]
  PLATFORM_NAME           = HelloRiscv
  PLATFORM_GUID           = 01234567-89ab-cdef-0123-456789abcdef
  PLATFORM_VERSION        = 1.0
  DSC_SPECIFICATION       = 0x00010005
  OUTPUT_DIRECTORY        = Build
  SUPPORTED_ARCHITECTURES = RISCV64
  BUILD_TARGETS           = DEBUG

[Components]
  HelloRiscv/HelloRiscv.inf

[Packages]
  MdePkg/MdePkg.dec
