cmake_minimum_required(VERSION 3.24)

set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")

project(gala-engine LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(LibtorrentRasterbar CONFIG REQUIRED)
find_package(nlohmann_json CONFIG REQUIRED)

add_executable(gala-engine src/main.cpp)
target_compile_options(gala-engine PRIVATE /utf-8 /W4)
target_compile_definitions(gala-engine PRIVATE NOMINMAX WIN32_LEAN_AND_MEAN)
target_link_libraries(gala-engine PRIVATE
  LibtorrentRasterbar::torrent-rasterbar
  nlohmann_json::nlohmann_json)
