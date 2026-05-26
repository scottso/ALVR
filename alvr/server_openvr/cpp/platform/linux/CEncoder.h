#pragma once

#include "alvr_server/IDRScheduler.h"
#include "shared/threadtools.h"
#include <atomic>
#include <memory>
#include <poll.h>
#include <sys/types.h>

class FrameRender;
class PoseHistory;

class CEncoder : public CThread {
public:
    CEncoder(std::shared_ptr<PoseHistory> poseHistory);
    ~CEncoder();
    bool Init() override { return true; }
    void Run() override;

    void Stop();
    void OnStreamStart();
    void InsertIDR();
    bool IsConnected() { return m_connected; }
    void CaptureFrame();

    // Forwards the foveation center to the live FrameRender, if any. Called from the C-ABI
    // entry point that the Rust tracking loop pumps on each gaze sample. Safe to call when
    // streaming hasn't started yet (drops silently).
    void UpdateFoveationCenter(float centerShiftX, float centerShiftY);

private:
    void GetFds(int client, int (*fds)[6]);
    std::shared_ptr<PoseHistory> m_poseHistory;
    std::atomic_bool m_exiting { false };
    IDRScheduler m_scheduler;
    pollfd m_socket;
    std::string m_socketPath;
    int m_fds[6];
    bool m_connected = false;
    std::atomic_bool m_captureFrame = false;
    std::atomic<FrameRender*> m_liveFrameRender { nullptr };
};
