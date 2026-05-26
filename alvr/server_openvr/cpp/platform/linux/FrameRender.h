#pragma once

#include "Renderer.h"
#include "ffmpeg_helper.h"
#include "protocol.h"

class FrameRender : public Renderer {
public:
    explicit FrameRender(alvr::VkContext& ctx, init_packet& init, int fds[]);
    ~FrameRender();

    Output CreateOutput();
    uint32_t GetEncodingWidth() const;
    uint32_t GetEncodingHeight() const;

private:
    struct ColorCorrection {
        float renderWidth;
        float renderHeight;
        float brightness;
        float contrast;
        float saturation;
        float gamma;
        float sharpening;
    };

    // Specialization constants. centerShift used to live here too but moved to
    // FoveationPushConstants so the warp center can follow gaze each frame.
    struct FoveationVars {
        float eyeWidthRatio;
        float eyeHeightRatio;
        float centerSizeX;
        float centerSizeY;
        float edgeRatioX;
        float edgeRatioY;
    };

    // Push-constant block laid out to match the `PushConstants` struct in ffr.comp.
    struct FoveationPushConstants {
        float centerShiftX;
        float centerShiftY;
    };

public:
    // Update the per-frame foveation center. Values are in normalized [-1, 1] coords and are
    // applied at the next Render() invocation. No-op if foveated encoding is disabled (the
    // foveation pipeline is never registered in that case).
    void UpdateFoveationCenter(float centerShiftX, float centerShiftY);

private:
    void setupColorCorrection();
    void setupFoveatedRendering();
    void setupCustomShaders(const std::string& stage);

    uint32_t m_width;
    uint32_t m_height;
    ExternalHandle m_handle = ExternalHandle::None;
    ColorCorrection m_colorCorrectionConstants;
    FoveationVars m_foveatedRenderingConstants;
    FoveationPushConstants m_foveatedRenderingPushConstants = { 0.0f, 0.0f };
    std::vector<RenderPipeline*> m_pipelines;
};
