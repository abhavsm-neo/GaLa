// gala-engine: a libtorrent process driven by JSON lines.
// A client connects to a named pipe and exchanges one JSON object per line. The engine
// keeps running when the client leaves, so seeding outlives the launcher's window.

#include <windows.h>

#include <libtorrent/add_torrent_params.hpp>
#include <libtorrent/alert_types.hpp>
#include <libtorrent/announce_entry.hpp>
#include <libtorrent/load_torrent.hpp>
#include <libtorrent/magnet_uri.hpp>
#include <libtorrent/peer_info.hpp>
#include <libtorrent/read_resume_data.hpp>
#include <libtorrent/session.hpp>
#include <libtorrent/session_params.hpp>
#include <libtorrent/torrent_handle.hpp>
#include <libtorrent/torrent_info.hpp>
#include <libtorrent/torrent_status.hpp>
#include <libtorrent/write_resume_data.hpp>

#include <nlohmann/json.hpp>

#include <algorithm>
#include <atomic>
#include <chrono>
#include <deque>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <mutex>
#include <set>
#include <sstream>
#include <string>
#include <thread>
#include <vector>

namespace lt = libtorrent;
namespace fs = std::filesystem;
using json = nlohmann::json;
using clock_type = std::chrono::steady_clock;

namespace {

constexpr wchar_t const* default_pipe = L"\\\\.\\pipe\\gala-engine";
constexpr DWORD pipe_buffer = 64 * 1024;
constexpr DWORD write_timeout_ms = 2000;
constexpr int exit_already_running = 3;

std::mutex queue_mutex;
std::deque<std::string> command_queue;

// One client at a time. Overlapped I/O is required: on a plain handle, a read waiting
// for the next command would block every write until the client spoke.
class Pipe {
public:
    explicit Pipe(std::wstring const& name) {
        handle_ = CreateNamedPipeW(name.c_str(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1, pipe_buffer, pipe_buffer, 0, nullptr);
        read_event_ = CreateEventW(nullptr, TRUE, FALSE, nullptr);
        write_event_ = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    }

    bool valid() const { return handle_ != INVALID_HANDLE_VALUE; }

    void write(std::string const& line) {
        std::lock_guard<std::mutex> lock(write_mutex_);
        if (!connected_) return;

        OVERLAPPED overlapped{};
        overlapped.hEvent = write_event_;
        DWORD written = 0;
        if (WriteFile(handle_, line.data(), static_cast<DWORD>(line.size()), nullptr, &overlapped)) return;
        if (GetLastError() != ERROR_IO_PENDING) return;
        if (WaitForSingleObject(write_event_, write_timeout_ms) == WAIT_OBJECT_0) return;

        // A client that stops reading must not freeze the torrents, so drop it.
        CancelIoEx(handle_, &overlapped);
        GetOverlappedResult(handle_, &overlapped, &written, TRUE);
        connected_ = false;
        DisconnectNamedPipe(handle_);
    }

    void serve() {
        for (;;) {
            if (!wait_for_client()) {
                std::this_thread::sleep_for(std::chrono::milliseconds(200));
                continue;
            }
            connected_ = true;
            read_lines();
            std::lock_guard<std::mutex> lock(write_mutex_);
            connected_ = false;
            DisconnectNamedPipe(handle_);
        }
    }

private:
    bool wait_for_client() {
        OVERLAPPED overlapped{};
        overlapped.hEvent = read_event_;
        if (ConnectNamedPipe(handle_, &overlapped)) return true;
        DWORD const error = GetLastError();
        if (error == ERROR_PIPE_CONNECTED) return true;
        if (error != ERROR_IO_PENDING) {
            DisconnectNamedPipe(handle_);
            return false;
        }
        DWORD ignored = 0;
        return GetOverlappedResult(handle_, &overlapped, &ignored, TRUE) != FALSE;
    }

    void read_lines() {
        std::string pending;
        std::vector<char> buffer(pipe_buffer);
        for (;;) {
            OVERLAPPED overlapped{};
            overlapped.hEvent = read_event_;
            DWORD count = 0;
            if (!ReadFile(handle_, buffer.data(), pipe_buffer, nullptr, &overlapped)
                && GetLastError() != ERROR_IO_PENDING) return;
            if (!GetOverlappedResult(handle_, &overlapped, &count, TRUE)) return;

            pending.append(buffer.data(), count);
            std::size_t end = 0;
            while ((end = pending.find('\n')) != std::string::npos) {
                std::string line = pending.substr(0, end);
                pending.erase(0, end + 1);
                if (line.empty()) continue;
                std::lock_guard<std::mutex> lock(queue_mutex);
                command_queue.push_back(std::move(line));
            }
        }
    }

    HANDLE handle_ = INVALID_HANDLE_VALUE;
    HANDLE read_event_ = nullptr;
    HANDLE write_event_ = nullptr;
    std::mutex write_mutex_;
    std::atomic<bool> connected_{false};
};

Pipe* client = nullptr;

void emit(json const& message) {
    if (client) client->write(message.dump() + '\n');
}

void emit_error(std::string const& id, std::string const& message) {
    emit({{"event", "error"}, {"id", id}, {"message", message}});
}

std::string to_id(lt::info_hash_t const& hashes) {
    std::ostringstream out;
    out << hashes.get_best();
    return out.str();
}

fs::path resume_path(fs::path const& state_dir, std::string const& id) {
    return state_dir / (id + ".resume");
}

std::vector<char> read_file(fs::path const& path) {
    std::ifstream in(path, std::ios::binary);
    return {std::istreambuf_iterator<char>(in), std::istreambuf_iterator<char>()};
}

// Replace the file in one step so a crash can't leave half a resume file behind.
void write_file(fs::path const& path, std::vector<char> const& data) {
    fs::path const temp = fs::path(path).concat(".tmp");
    {
        std::ofstream out(temp, std::ios::binary | std::ios::trunc);
        out.write(data.data(), static_cast<std::streamsize>(data.size()));
    }
    fs::rename(temp, path);
}

constexpr int piece_map_cells = 600;
constexpr lt::status_flags_t status_fields = lt::torrent_handle::query_name | lt::torrent_handle::query_save_path;

char const* state_name(lt::torrent_status::state_t state) {
    switch (state) {
        case lt::torrent_status::checking_files: return "checking";
        case lt::torrent_status::downloading_metadata: return "metadata";
        case lt::torrent_status::downloading: return "downloading";
        case lt::torrent_status::finished: return "finished";
        case lt::torrent_status::seeding: return "seeding";
        case lt::torrent_status::checking_resume_data: return "checking";
        default: return "unknown";
    }
}

json to_json(lt::torrent_status const& status) {
    bool const paused = (status.flags & lt::torrent_flags::paused) != lt::torrent_flags_t{};
    bool const managed = (status.flags & lt::torrent_flags::auto_managed) != lt::torrent_flags_t{};
    return {
        {"event", "progress"},
        {"id", to_id(status.info_hashes)},
        {"name", status.name},
        {"state", state_name(status.state)},
        // A torrent the queue is holding back is paused but still managed.
        {"paused", paused && !managed},
        {"queued", paused && managed},
        {"queue_position", static_cast<int>(status.queue_position)},
        {"error", status.errc ? status.errc.message() : ""},
        {"save_path", status.save_path},
        {"added_time", static_cast<std::int64_t>(status.added_time)},
        {"all_time_download", status.all_time_download},
        {"all_time_upload", status.all_time_upload},
        {"seeding_seconds", status.seeding_duration.count()},
        {"progress", status.progress},
        {"done_bytes", status.total_done},
        {"total_bytes", status.total_wanted},
        {"down_rate", paused ? 0 : status.download_payload_rate},
        {"up_rate", paused ? 0 : status.upload_payload_rate},
        {"peers", status.num_peers},
        {"seeds", status.num_seeds},
    };
}

class Engine {
public:
    explicit Engine(fs::path state_dir) : state_dir_(std::move(state_dir)), session_(make_params()) {
        load_resume_files();
    }

    bool handle_command(std::string const& line) {
        json command;
        try {
            command = json::parse(line);
        } catch (json::exception const& e) {
            emit_error("", std::string("Bad command: ") + e.what());
            return true;
        }

        std::string const name = command.value("cmd", "");
        if (name == "shutdown") return false;

        try {
            if (name == "add") add(command);
            else if (name == "snapshot") snapshot();
            else if (name == "detail") detail(command.at("id").get<std::string>());
            else if (name == "set_limits") set_limits(command);
            else if (name == "start_next") find(command.at("id").get<std::string>()).queue_position_top();
            else if (name == "pause") pause(command.at("id").get<std::string>());
            else if (name == "resume") resume(command.at("id").get<std::string>());
            else if (name == "remove") remove(command.at("id").get<std::string>(), command.value("delete_files", false));
            else emit_error("", "Unknown command: " + name);
        } catch (std::exception const& e) {
            emit_error(command.value("id", ""), e.what());
        }
        return true;
    }

    void tick() {
        auto const now = clock_type::now();
        if (now >= next_status_) {
            session_.post_torrent_updates();
            next_status_ = now + std::chrono::seconds(1);
        }
        if (now >= next_save_) {
            for (auto const& [id, handle] : handles_) request_save(handle);
            next_save_ = now + std::chrono::seconds(30);
        }
        session_.wait_for_alert(std::chrono::milliseconds(100));
        handle_alerts();
    }

    void shut_down() {
        session_.pause();
        handle_alerts();
        // Saves for removed torrents never report back, so start the count fresh.
        pending_saves_ = 0;
        for (auto const& [id, handle] : handles_) request_save(handle);

        auto const deadline = clock_type::now() + std::chrono::seconds(10);
        while (pending_saves_ > 0 && clock_type::now() < deadline) {
            session_.wait_for_alert(std::chrono::milliseconds(100));
            handle_alerts();
        }
        emit({{"event", "stopped"}});
    }

private:
    // pread_disk_io holds finished pieces in memory, so a crash loses most of a large
    // download. The default memory mapped storage marks pieces done as they complete.
    static lt::session_params make_params() {
        return lt::session_params(make_settings());
    }

    static lt::settings_pack make_settings() {
        lt::settings_pack settings;
        settings.set_int(lt::settings_pack::stop_tracker_timeout, 2);
        // One game at a time. libtorrent still starts the next one if this one stalls.
        settings.set_int(lt::settings_pack::active_downloads, 1);
        settings.set_int(lt::settings_pack::alert_mask,
            lt::alert_category::status | lt::alert_category::error | lt::alert_category::storage);
        settings.set_bool(lt::settings_pack::enable_dht, true);
        settings.set_bool(lt::settings_pack::enable_lsd, true);
        settings.set_str(lt::settings_pack::user_agent, "GaLa/0.1");
        return settings;
    }

    void load_resume_files() {
        for (auto const& entry : fs::directory_iterator(state_dir_)) {
            if (entry.path().extension() != ".resume") continue;
            lt::error_code error;
            auto const data = read_file(entry.path());
            lt::add_torrent_params params = lt::read_resume_data(data, error);
            if (error) {
                emit_error("", "Couldn't read " + entry.path().filename().string() + ": " + error.message());
                continue;
            }
            session_.async_add_torrent(std::move(params));
        }
    }

    void add(json const& command) {
        std::string const save_path = command.at("save_path").get<std::string>();
        fs::create_directories(fs::u8path(save_path));

        lt::add_torrent_params params;
        if (command.contains("magnet")) {
            params = lt::parse_magnet_uri(command.at("magnet").get<std::string>());
        } else {
            params = lt::load_torrent_file(command.at("torrent_file").get<std::string>());
        }
        params.save_path = save_path;
        session_.async_add_torrent(std::move(params));
    }

    // A client that just connected has missed everything, so send the full picture.
    void snapshot() {
        emit({{"event", "ready"}});
        for (auto const& [id, handle] : handles_) {
            if (handle.is_valid()) emit_status(handle.status(status_fields));
        }
    }

    // libtorrent calls a torrent finished once every piece is verified, which can be a
    // minute before the last of it reaches the disk on a large download.
    void emit_status(lt::torrent_status const& status) {
        json message = to_json(status);
        if (flushing_.count(message["id"].get<std::string>())) message["state"] = "writing";
        emit(message);
    }

    void set_limits(json const& command) {
        lt::settings_pack settings;
        settings.set_int(lt::settings_pack::download_rate_limit, command.value("down", 0));
        settings.set_int(lt::settings_pack::upload_rate_limit, command.value("up", 0));
        session_.apply_settings(std::move(settings));
    }

    void detail(std::string const& id) {
        auto const& handle = find(id);

        std::vector<lt::peer_info> peer_list;
        handle.get_peer_info(peer_list);
        json peers = json::array();
        for (auto const& peer : peer_list) {
            peers.push_back({
                {"ip", peer.remote_endpoint().address().to_string()},
                {"client", peer.client},
                {"down_rate", peer.payload_down_speed},
                {"up_rate", peer.payload_up_speed},
                {"progress", peer.progress},
            });
        }

        json files = json::array();
        if (auto const info = handle.torrent_file()) {
            auto const& layout = info->layout();
            auto const done = handle.file_progress(lt::torrent_handle::piece_granularity);
            int index = 0;
            for (auto const file : layout.file_range()) {
                files.push_back({
                    {"path", layout.file_path(file)},
                    {"size", layout.file_size(file)},
                    {"done", done[static_cast<std::size_t>(index++)]},
                });
            }
        }

        json trackers = json::array();
        for (auto const& tracker : handle.trackers()) {
            trackers.push_back({{"url", tracker.url}, {"verified", tracker.verified}});
        }

        // Fold the pieces into a fixed number of cells for the piece map.
        auto const status = handle.status(lt::torrent_handle::query_pieces);
        int const total = status.pieces.size();
        int const cells = std::min(total, piece_map_cells);
        json pieces = json::array();
        for (int cell = 0; cell < cells; ++cell) {
            int const first = static_cast<int>(std::int64_t{cell} * total / cells);
            int const last = static_cast<int>(std::int64_t{cell + 1} * total / cells);
            int have = 0;
            for (int piece = first; piece < last; ++piece) {
                if (status.pieces[lt::piece_index_t{piece}]) ++have;
            }
            pieces.push_back(static_cast<float>(have) / static_cast<float>(last - first));
        }

        emit({{"event", "detail"}, {"id", id}, {"peers", peers}, {"files", files},
            {"trackers", trackers}, {"pieces", pieces}});
    }

    lt::torrent_handle const& find(std::string const& id) const {
        auto const found = handles_.find(id);
        if (found == handles_.end()) throw std::runtime_error("No torrent with that id");
        return found->second;
    }

    void pause(std::string const& id) {
        auto const& handle = find(id);
        handle.unset_flags(lt::torrent_flags::auto_managed);
        handle.pause();
        request_save(handle);
    }

    void resume(std::string const& id) {
        auto const& handle = find(id);
        handle.clear_error();
        handle.set_flags(lt::torrent_flags::auto_managed);
        handle.resume();
        request_save(handle);
    }

    void remove(std::string const& id, bool delete_files) {
        auto const handle = find(id);
        handles_.erase(id);
        session_.remove_torrent(handle, delete_files ? lt::session::delete_files : lt::remove_flags_t{});
        std::error_code ignored;
        fs::remove(resume_path(state_dir_, id), ignored);
        emit({{"event", "removed"}, {"id", id}});
    }

    void request_save(lt::torrent_handle const& handle) {
        if (!handle.is_valid()) return;
        handle.save_resume_data(lt::torrent_handle::save_info_dict | lt::torrent_handle::flush_disk_cache);
        ++pending_saves_;
    }

    void finish_save() {
        if (pending_saves_ > 0) --pending_saves_;
    }

    void handle_alerts() {
        std::vector<lt::alert*> alerts;
        session_.pop_alerts(&alerts);

        for (lt::alert* alert : alerts) {
            if (auto* added = lt::alert_cast<lt::add_torrent_alert>(alert)) {
                if (added->error) {
                    emit_error("", added->error.message());
                    continue;
                }
                std::string const id = to_id(added->handle.info_hashes());
                handles_[id] = added->handle;
                request_save(added->handle);
                emit({{"event", "added"}, {"id", id}});
                emit_status(added->handle.status(status_fields));
            } else if (auto* update = lt::alert_cast<lt::state_update_alert>(alert)) {
                for (auto const& status : update->status) emit_status(status);
            } else if (auto* finished = lt::alert_cast<lt::torrent_finished_alert>(alert)) {
                // Installs read these files next, so only report finished once they're on disk.
                flushing_.insert(to_id(finished->handle.info_hashes()));
                finished->handle.flush_cache();
                request_save(finished->handle);
            } else if (auto* flushed = lt::alert_cast<lt::cache_flushed_alert>(alert)) {
                std::string const id = to_id(flushed->handle.info_hashes());
                if (flushing_.erase(id)) {
                    emit({{"event", "finished"}, {"id", id}});
                    emit_status(flushed->handle.status(status_fields));
                }
            } else if (auto* metadata = lt::alert_cast<lt::metadata_received_alert>(alert)) {
                request_save(metadata->handle);
            } else if (auto* saved = lt::alert_cast<lt::save_resume_data_alert>(alert)) {
                std::string const id = to_id(saved->params.info_hashes);
                if (handles_.count(id)) {
                    write_file(resume_path(state_dir_, id), lt::write_resume_data_buf(saved->params));
                    emit({{"event", "saved"}, {"id", id}, {"pieces", saved->params.have_pieces.count()}});
                }
                finish_save();
            } else if (lt::alert_cast<lt::save_resume_data_failed_alert>(alert)) {
                finish_save();
            } else if (auto* rejected = lt::alert_cast<lt::fastresume_rejected_alert>(alert)) {
                emit({{"event", "resume_rejected"},
                    {"id", to_id(rejected->handle.info_hashes())},
                    {"message", rejected->error.message()}});
            } else if (auto* undeleted = lt::alert_cast<lt::torrent_delete_failed_alert>(alert)) {
                emit_error(to_id(undeleted->info_hashes), "Couldn't delete the files: " + undeleted->error.message());
            } else if (auto* failed = lt::alert_cast<lt::torrent_error_alert>(alert)) {
                emit_error(to_id(failed->handle.info_hashes()), failed->error.message());
            }
        }
    }

    fs::path state_dir_;
    lt::session session_;
    std::map<std::string, lt::torrent_handle> handles_;
    std::set<std::string> flushing_;
    int pending_saves_ = 0;
    clock_type::time_point next_status_ = clock_type::now();
    clock_type::time_point next_save_ = clock_type::now() + std::chrono::seconds(30);
};

} // namespace

int wmain(int argc, wchar_t** argv) {
    if (argc != 2 && argc != 3) {
        std::cerr << "Usage: gala-engine <state-folder> [pipe-name]\n";
        return 2;
    }

    try {
        // Creating the pipe first means a second engine can't touch the first one's torrents.
        Pipe pipe(argc == 3 ? argv[2] : default_pipe);
        if (!pipe.valid()) {
            std::cerr << "Another engine is already running\n";
            return exit_already_running;
        }
        client = &pipe;

        fs::path const state_dir = argv[1];
        fs::create_directories(state_dir);

        Engine engine(state_dir);
        std::thread([&pipe] { pipe.serve(); }).detach();

        bool running = true;
        while (running) {
            std::deque<std::string> commands;
            {
                std::lock_guard<std::mutex> lock(queue_mutex);
                commands.swap(command_queue);
            }
            for (auto const& command : commands) {
                if (!engine.handle_command(command)) running = false;
            }
            if (running) engine.tick();
        }

        engine.shut_down();

        // Everything is saved. Saying goodbye to trackers and DHT nodes can take half a
        // minute, and a client waiting to relaunch shouldn't sit through that.
        std::thread([] {
            std::this_thread::sleep_for(std::chrono::seconds(5));
            ExitProcess(0);
        }).detach();
    } catch (std::exception const& e) {
        emit_error("", e.what());
        std::cerr << e.what() << '\n';
        return 1;
    }
    return 0;
}
