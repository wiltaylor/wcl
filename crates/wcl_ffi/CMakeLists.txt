cmake_minimum_required(VERSION 3.14)
project(wcl VERSION 0.0.0 LANGUAGES C)

# ── Locate header ───────────────────────────────────────────────────────
# Packaged archive: include/wcl.h    Source tree: wcl.h in current dir

if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/include/wcl.h")
    set(_include_dir "${CMAKE_CURRENT_SOURCE_DIR}/include")
elseif(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/wcl.h")
    set(_include_dir "${CMAKE_CURRENT_SOURCE_DIR}")
else()
    message(FATAL_ERROR "Cannot find wcl.h in include/ or current directory")
endif()

# ── Locate prebuilt static library ──────────────────────────────────────
# Users may set WCL_LIB_DIR explicitly. Otherwise we auto-detect by
# platform from lib/<platform>/ (packaged archive or source tree) or
# from the cargo output in target/release/.

if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    if(CMAKE_SYSTEM_PROCESSOR MATCHES "x86_64|AMD64")
        set(_platform "linux_amd64")
    elseif(CMAKE_SYSTEM_PROCESSOR MATCHES "aarch64|ARM64")
        set(_platform "linux_arm64")
    endif()
elseif(CMAKE_SYSTEM_NAME STREQUAL "Darwin")
    if(CMAKE_SYSTEM_PROCESSOR MATCHES "x86_64|AMD64")
        set(_platform "darwin_amd64")
    elseif(CMAKE_SYSTEM_PROCESSOR MATCHES "aarch64|arm64|ARM64")
        set(_platform "darwin_arm64")
    endif()
elseif(CMAKE_SYSTEM_NAME STREQUAL "Windows")
    if(CMAKE_SYSTEM_PROCESSOR MATCHES "x86_64|AMD64")
        set(_platform "windows_amd64")
    endif()
endif()

if(NOT _platform)
    message(FATAL_ERROR "Unsupported platform: ${CMAKE_SYSTEM_NAME} ${CMAKE_SYSTEM_PROCESSOR}")
endif()

if(NOT WCL_LIB_DIR)
    find_library(WCL_LIBRARY
        NAMES wcl_ffi libwcl_ffi
        PATHS
            "${CMAKE_CURRENT_SOURCE_DIR}/lib/${_platform}"
            "${CMAKE_CURRENT_SOURCE_DIR}/../../bindings/zig/lib/${_platform}"
            "${CMAKE_CURRENT_SOURCE_DIR}/../../target/release"
        NO_DEFAULT_PATH
    )

    if(WCL_LIBRARY)
        get_filename_component(WCL_LIB_DIR "${WCL_LIBRARY}" DIRECTORY)
    else()
        message(FATAL_ERROR
            "Could not find the wcl_ffi static library for ${_platform}.\n"
            "Build it first with: cargo build -p wcl_ffi --release\n"
            "Or set -DWCL_LIB_DIR=<path> to the directory containing the library.")
    endif()
endif()

# ── Create the imported target ──────────────────────────────────────────

add_library(wcl::wcl STATIC IMPORTED GLOBAL)

if(WIN32)
    set(_lib_name "wcl_ffi.lib")
else()
    set(_lib_name "libwcl_ffi.a")
endif()

set_target_properties(wcl::wcl PROPERTIES
    IMPORTED_LOCATION "${WCL_LIB_DIR}/${_lib_name}"
    INTERFACE_INCLUDE_DIRECTORIES "${_include_dir}"
)

# ── Link platform dependencies ──────────────────────────────────────────

if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    find_package(Threads REQUIRED)
    target_link_libraries(wcl::wcl INTERFACE Threads::Threads m dl)
elseif(CMAKE_SYSTEM_NAME STREQUAL "Darwin")
    find_package(Threads REQUIRED)
    target_link_libraries(wcl::wcl INTERFACE Threads::Threads m dl "-framework Security")
elseif(CMAKE_SYSTEM_NAME STREQUAL "Windows")
    target_link_libraries(wcl::wcl INTERFACE ws2_32 bcrypt userenv)
endif()
