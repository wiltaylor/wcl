cmake_minimum_required(VERSION 3.14)
project(wcl VERSION 0.0.0 LANGUAGES C CXX)

# ── Find the prebuilt static library ────────────────────────────────────
# Users set WCL_LIB_DIR to the directory containing the prebuilt library,
# or WCL_ROOT to the wcl_ffi crate root (this directory) for auto-detection.

if(NOT WCL_LIB_DIR)
    if(WCL_ROOT)
        set(_root "${WCL_ROOT}")
    else()
        set(_root "${CMAKE_CURRENT_SOURCE_DIR}")
    endif()

    # When built with `cargo build -p wcl_ffi --release`, the library lands
    # in target/release/. When cross-compiled or used from the Zig bindings,
    # it may be in a platform subdirectory.
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

    # Search order: explicit lib/ subdirectory, Zig binding layout, cargo target/release/
    find_library(WCL_LIBRARY
        NAMES wcl_ffi libwcl_ffi
        PATHS
            "${_root}/lib/${_platform}"
            "${_root}/../../bindings/zig/lib/${_platform}"
            "${_root}/../../target/release"
        NO_DEFAULT_PATH
    )

    if(WCL_LIBRARY)
        get_filename_component(WCL_LIB_DIR "${WCL_LIBRARY}" DIRECTORY)
    else()
        message(FATAL_ERROR
            "Could not find the wcl_ffi static library.\n"
            "Build it first with: cargo build -p wcl_ffi --release\n"
            "Or set -DWCL_LIB_DIR=<path> to the directory containing libwcl_ffi.a / wcl_ffi.lib")
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
    INTERFACE_INCLUDE_DIRECTORIES "${CMAKE_CURRENT_SOURCE_DIR}"
)

# ── Link platform dependencies ──────────────────────────────────────────

if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    find_package(Threads REQUIRED)
    target_link_libraries(wcl::wcl INTERFACE
        Threads::Threads
        m
        dl
    )
elseif(CMAKE_SYSTEM_NAME STREQUAL "Darwin")
    find_package(Threads REQUIRED)
    target_link_libraries(wcl::wcl INTERFACE
        Threads::Threads
        m
        dl
        "-framework Security"
    )
elseif(CMAKE_SYSTEM_NAME STREQUAL "Windows")
    target_link_libraries(wcl::wcl INTERFACE
        ws2_32
        bcrypt
        userenv
    )
endif()
