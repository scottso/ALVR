#pragma once

#include "d3d-render-utils/RenderPipeline.h"

class FFR {
public:
    FFR(ID3D11Device* device);
    void Initialize(ID3D11Texture2D* compositionTexture);
    void Render();
    void GetOptimizedResolution(uint32_t* width, uint32_t* height);
    ID3D11Texture2D* GetOutputTexture();

    // Push a new foveation warp center to the cbuffer the FFR pixel shader reads. Values
    // are in normalized [-1, 1] coords. No-op when foveated encoding is disabled (the
    // pipeline wasn't created and the cbuffer doesn't exist).
    void UpdateCenterShift(float centerShiftX, float centerShiftY);

private:
    Microsoft::WRL::ComPtr<ID3D11Device> mDevice;
    // Cached at Initialize() so UpdateCenterShift doesn't refcount the immediate context
    // every frame. Only touched by the compositor thread (see CEncoder::CopyToStaging).
    Microsoft::WRL::ComPtr<ID3D11DeviceContext> mContext;
    Microsoft::WRL::ComPtr<ID3D11Texture2D> mOptimizedTexture;
    Microsoft::WRL::ComPtr<ID3D11VertexShader> mQuadVertexShader;
    // Holds the same buffer the pipeline binds — updating its contents via
    // UpdateSubresource lands on the next GPU dispatch without rebuilding anything.
    Microsoft::WRL::ComPtr<ID3D11Buffer> mFoveatedRenderingBuffer;

    std::vector<d3d_render_utils::RenderPipeline> mPipelines;
};
