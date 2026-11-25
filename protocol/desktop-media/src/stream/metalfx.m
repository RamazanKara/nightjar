// MetalFX operates independently of SDL. Only IOSurfaces cross the boundary.
#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#import <MetalFX/MetalFX.h>
#import <QuartzCore/CAMetalLayer.h>
#import <CoreVideo/CoreVideo.h>
#import <IOSurface/IOSurfaceRef.h>

// Output images SDL draws. Rust pools one SDL_Texture per ring slot, so slots rotate round-robin
// without checking IOSurface use counts (pooled textures keep the IOSurface referenced). When SDL's
// shared MTLCommandQueue is provided, MetalFX submits asynchronously and relies on same-queue FIFO
// ordering with SDL's subsequent draw, retaining the input CVPixelBufferRef/textures until GPU
// completion; otherwise it falls back to a private queue with waitUntilCompleted.
#define NJ_RING 4

@interface NJSpatial : NSObject {
@public
    CVPixelBufferRef ring[NJ_RING];
    __strong id<MTLTexture> targets[NJ_RING];
    __strong id<MTLCommandBuffer> inFlight[NJ_RING];
    BOOL sharedQueue;
    int next;
    unsigned int frames;
    double lastGpuMs;
    NSLock *gpuMsLock;
}
@property id<MTLDevice> device;
@property id<MTLCommandQueue> queue;
@property id<MTLComputePipelineState> convert;
@property id<MTLFXSpatialScaler> scaler;
@property id<MTLTexture> rgb;
@property id<MTLTexture> scaled;
@property CVMetalTextureCacheRef cache;
@end
@implementation NJSpatial
- (void)dealloc {
    for (int i = 0; i < NJ_RING; i++) {
        if (inFlight[i]) {
            [inFlight[i] waitUntilCompleted];
            inFlight[i] = nil;
        }
    }
    if (_cache) CFRelease(_cache);
    for (int i = 0; i < NJ_RING; i++) if (ring[i]) CFRelease(ring[i]);
}
@end

// A BGRA IOSurface image the scaler writes and SDL's Metal renderer reads.
static bool nj_new_output(NJSpatial *s, CVPixelBufferRef *buffer, id<MTLTexture> __strong *texture) {
    CVPixelBufferRef b=NULL;
    NSDictionary *attributes=@{(id)kCVPixelBufferIOSurfacePropertiesKey:@{},(id)kCVPixelBufferMetalCompatibilityKey:@YES};
    if (CVPixelBufferCreate(NULL,s.scaler.outputWidth,s.scaler.outputHeight,kCVPixelFormatType_32BGRA,(__bridge CFDictionaryRef)attributes,&b)!=kCVReturnSuccess) return false;
    MTLTextureDescriptor *d=[MTLTextureDescriptor texture2DDescriptorWithPixelFormat:MTLPixelFormatBGRA8Unorm width:s.scaler.outputWidth height:s.scaler.outputHeight mipmapped:NO];
    d.usage=MTLTextureUsageShaderRead;
    id<MTLTexture> t=[s.device newTextureWithDescriptor:d iosurface:CVPixelBufferGetIOSurface(b) plane:0];
    if (!t) { CFRelease(b); return false; }
    *buffer=b; *texture=t;
    return true;
}

bool nj_metalfx_supported(void) {
    @autoreleasepool {
        if (@available(macOS 13.0, *)) {
            id<MTLDevice> device=MTLCreateSystemDefaultDevice();
            return device && [MTLFXSpatialScalerDescriptor supportsDevice:device];
        }
        return false;
    }
}

// The conversion kernel is the same for every size and scaler, so it is compiled once per device:
// a window resize builds a new scaler, and newLibraryWithSource there compiled it again on the
// presenting thread.
static id<MTLComputePipelineState> nj_conversion_pipeline(id<MTLDevice> device) {
    static NSMutableDictionary<NSNumber *, id<MTLComputePipelineState>> *pipelines;
    static NSLock *lock;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        pipelines=[NSMutableDictionary new];
        lock=[NSLock new];
    });
    NSNumber *key=@(device.registryID);
    [lock lock];
    id<MTLComputePipelineState> pipeline=pipelines[key];
    [lock unlock];
    if (pipeline) return pipeline;
    // Inputs are normalized NV12/P010. Range and matrix come from AVFrame, not host brand.
    NSString *source = @"#include <metal_stdlib>\nusing namespace metal;\n"
    "kernel void yuv(texture2d<float, access::sample> y [[texture(0)]], texture2d<float, access::sample> uv [[texture(1)]], texture2d<float, access::write> rgb [[texture(2)]], constant float4 *p [[buffer(0)]], uint2 xy [[thread_position_in_grid]]) {"
    "if (xy.x >= rgb.get_width() || xy.y >= rgb.get_height()) return;"
    "constexpr sampler sm(coord::normalized, address::clamp_to_edge, filter::linear);"
    "float2 pos=(float2(xy)+0.5)/float2(y.get_width(),y.get_height());"
    "float l=(y.sample(sm,pos).r-p[0].x)*p[0].y; float2 c=(uv.sample(sm,pos).rg-p[0].z)*p[0].w;"
    "rgb.write(float4(clamp(float3(l+p[1].x*c.y,l+p[1].y*c.x+p[1].z*c.y,l+p[1].w*c.x),0.0,1.0),1),xy);}";
    NSError *error = nil;
    id<MTLLibrary> library = [device newLibraryWithSource:source options:nil error:&error];
    if (!library) { NSLog(@"Nightjar MetalFX conversion: %@", error); return nil; }
    pipeline = [device newComputePipelineStateWithFunction:[library newFunctionWithName:@"yuv"] error:&error];
    if (!pipeline) { NSLog(@"Nightjar MetalFX conversion: %@", error); return nil; }
    [lock lock];
    // Another stream may have built it meanwhile; keep one pipeline per device.
    if (pipelines[key]) pipeline=pipelines[key]; else pipelines[key]=pipeline;
    [lock unlock];
    return pipeline;
}

void *nj_metalfx_new(void *layer, void *shared_queue, int iw, int ih, int ow, int oh) {
    @autoreleasepool {
        if (@available(macOS 13.0, *)) {
            if (!layer || iw <= 0 || ih <= 0 || ow < iw || oh < ih) return NULL;
            NJSpatial *s = [NJSpatial new];
            s.device = ((__bridge CAMetalLayer *)layer).device;
            if (!s.device || ![MTLFXSpatialScalerDescriptor supportsDevice:s.device]) return NULL;
            s->gpuMsLock = [NSLock new];
            if (shared_queue) {
                s.queue = (__bridge id<MTLCommandQueue>)shared_queue;
                s->sharedQueue = YES;
            } else {
                s.queue = [s.device newCommandQueue];
                s->sharedQueue = NO;
            }
            s.convert = nj_conversion_pipeline(s.device);
            MTLFXSpatialScalerDescriptor *d = [MTLFXSpatialScalerDescriptor new];
            d.inputWidth=iw; d.inputHeight=ih; d.outputWidth=ow; d.outputHeight=oh;
            d.colorTextureFormat=MTLPixelFormatBGRA8Unorm;
            d.outputTextureFormat=MTLPixelFormatBGRA8Unorm;
            d.colorProcessingMode=MTLFXSpatialScalerColorProcessingModePerceptual;
            s.scaler=[d newSpatialScalerWithDevice:s.device];
            if (!s.scaler || !s.convert || !s.queue) return NULL;
            MTLTextureDescriptor *t=[MTLTextureDescriptor texture2DDescriptorWithPixelFormat:d.colorTextureFormat width:iw height:ih mipmapped:NO];
            t.storageMode=MTLStorageModePrivate;
            t.usage=s.scaler.colorTextureUsage|MTLTextureUsageShaderWrite;
            s.rgb=[s.device newTextureWithDescriptor:t];
            // MetalFX writes only private textures; a blit copies the result into an IOSurface image.
            t.width=ow; t.height=oh; t.usage=s.scaler.outputTextureUsage;
            s.scaled=[s.device newTextureWithDescriptor:t];
            CVMetalTextureCacheRef cache=NULL;
            if (CVMetalTextureCacheCreate(NULL,NULL,s.device,NULL,&cache)!=kCVReturnSuccess) return NULL;
            s.cache=cache;
            if (!s.rgb || !s.scaled) return NULL;
            return (__bridge_retained void *)s;
        }
        return NULL;
    }
}

// Writes into the next ring slot and returns its index (0..NJ_RING-1), or -1 on failure. There is
// no CPU pixel readback; the wait keeps SDL's queue from reading the image before it is written.
int nj_metalfx_draw(void *context, void *input, const float *parameters, double *gpu_ms) {
    @autoreleasepool {
        NJSpatial *s = (__bridge NJSpatial *)context;
        CVPixelBufferRef buffer = input;
        if (!buffer || CVPixelBufferGetPlaneCount(buffer) != 2) return -1;
        OSType format = CVPixelBufferGetPixelFormatType(buffer);
        bool ten = format == kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange
            || format == kCVPixelFormatType_420YpCbCr10BiPlanarFullRange;
        if (!ten && format != kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
            && format != kCVPixelFormatType_420YpCbCr8BiPlanarFullRange) return -1;
        CVMetalTextureRef planes[2] = {NULL, NULL};
        int slot = s->next;
        int result = -1;
        if (s->inFlight[slot]) {
            [s->inFlight[slot] waitUntilCompleted];
            s->inFlight[slot] = nil;
        }
        for (int i = 0; i < 2; i++) {
            MTLPixelFormat f = i ? (ten ? MTLPixelFormatRG16Unorm : MTLPixelFormatRG8Unorm)
                : (ten ? MTLPixelFormatR16Unorm : MTLPixelFormatR8Unorm);
            if (CVMetalTextureCacheCreateTextureFromImage(
                    NULL, s.cache, buffer, NULL, f,
                    CVPixelBufferGetWidthOfPlane(buffer, i),
                    CVPixelBufferGetHeightOfPlane(buffer, i), i, &planes[i]) != kCVReturnSuccess) {
                goto cleanup;
            }
        }
        if (!s->ring[slot] && !nj_new_output(s, &s->ring[slot], &s->targets[slot])) {
            goto cleanup;
        }
        {
            id<MTLCommandBuffer> command = [s.queue commandBuffer];
            if (!command) {
                goto cleanup;
            }
            id<MTLComputeCommandEncoder> e = [command computeCommandEncoder];
            [e setComputePipelineState:s.convert];
            [e setTexture:CVMetalTextureGetTexture(planes[0]) atIndex:0];
            [e setTexture:CVMetalTextureGetTexture(planes[1]) atIndex:1];
            [e setTexture:s.rgb atIndex:2];
            [e setBytes:parameters length:8 * sizeof(float) atIndex:0];
            [e dispatchThreads:MTLSizeMake(s.rgb.width, s.rgb.height, 1)
         threadsPerThreadgroup:MTLSizeMake(8, 8, 1)];
            [e endEncoding];
            s.scaler.colorTexture = s.rgb;
            s.scaler.outputTexture = s.scaled;
            s.scaler.inputContentWidth = s.rgb.width;
            s.scaler.inputContentHeight = s.rgb.height;
            [s.scaler encodeToCommandBuffer:command];
            id<MTLBlitCommandEncoder> blit = [command blitCommandEncoder];
            [blit copyFromTexture:s.scaled toTexture:s->targets[slot]];
            [blit endEncoding];
            if (s->sharedQueue) {
                CFRetain(buffer);
                CVMetalTextureRef p0 = planes[0];
                CVMetalTextureRef p1 = planes[1];
                planes[0] = NULL;
                planes[1] = NULL;
                __weak NJSpatial *weakSelf = s;
                [command addCompletedHandler:^(id<MTLCommandBuffer> cb) {
                    CFRelease(p0);
                    CFRelease(p1);
                    CFRelease(buffer);
                    if (cb.status == MTLCommandBufferStatusError) {
                        NSLog(@"Nightjar MetalFX: %@", cb.error);
                    } else {
                        NJSpatial *strongSelf = weakSelf;
                        if (strongSelf) {
                            double ms = (cb.GPUEndTime - cb.GPUStartTime) * 1000.0;
                            [strongSelf->gpuMsLock lock];
                            strongSelf->lastGpuMs = ms;
                            [strongSelf->gpuMsLock unlock];
                        }
                    }
                }];
                s->inFlight[slot] = command;
                [command commit];
                s->next = (slot + 1) % NJ_RING;
                result = slot;
                if (gpu_ms) {
                    [s->gpuMsLock lock];
                    *gpu_ms = s->lastGpuMs;
                    [s->gpuMsLock unlock];
                }
            } else {
                [command commit];
                [command waitUntilCompleted];
                if (command.status == MTLCommandBufferStatusError) {
                    NSLog(@"Nightjar MetalFX: %@", command.error);
                } else {
                    s->next = (slot + 1) % NJ_RING;
                    result = slot;
                }
                double ms = (command.GPUEndTime - command.GPUStartTime) * 1000.0;
                [s->gpuMsLock lock];
                s->lastGpuMs = ms;
                if (gpu_ms) *gpu_ms = ms;
                [s->gpuMsLock unlock];
            }
        }
cleanup:
        for (int i = 0; i < 2; i++) if (planes[i]) CFRelease(planes[i]);
        if (++s->frames >= 60) {
            s->frames = 0;
            CVMetalTextureCacheFlush(s.cache, 0);
        }
        return result;
    }
}

void *nj_metalfx_slot_buffer(void *context, int slot) {
    if (!context || slot < 0 || slot >= NJ_RING) return NULL;
    NJSpatial *s = (__bridge NJSpatial *)context;
    return s->ring[slot];
}

void nj_metalfx_free(void *context) {
    @autoreleasepool {
        if (context) {
            NJSpatial *s = (__bridge NJSpatial *)context;
            for (int i = 0; i < NJ_RING; i++) {
                if (s->inFlight[i]) {
                    [s->inFlight[i] waitUntilCompleted];
                    s->inFlight[i] = nil;
                }
            }
        }
        (void)CFBridgingRelease(context);
    }
}
