#import <AVFoundation/AVFoundation.h>
#import <CoreVideo/CoreVideo.h>

// All entry points and the end notification run on the application's main thread.
@interface MDPlayer : NSObject
@property AVPlayer *player;
@property AVPlayerItemVideoOutput *output;
@property id observer;
@property BOOL ended;
@end
@implementation MDPlayer
- (void)dealloc {
    if (_observer) [[NSNotificationCenter defaultCenter] removeObserver:_observer];
    [_player pause];
}
@end

void *md_player_create(const char *path) {
    @autoreleasepool {
        NSString *filename = [[NSFileManager defaultManager] stringWithFileSystemRepresentation:path length:strlen(path)];
        if (!filename) return NULL;
        AVURLAsset *asset = [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:filename] options:nil];
        AVPlayerItem *item = [AVPlayerItem playerItemWithAsset:asset];
        // Applies the source track's preferred transform (portrait / rotated clips).
        item.videoComposition = [AVVideoComposition videoCompositionWithPropertiesOfAsset:asset];
        MDPlayer *state = [MDPlayer new];
        state.output = [[AVPlayerItemVideoOutput alloc] initWithPixelBufferAttributes:@{
            (NSString *)kCVPixelBufferPixelFormatTypeKey: @(kCVPixelFormatType_420YpCbCr8BiPlanarFullRange),
            (NSString *)kCVPixelBufferIOSurfacePropertiesKey: @{},
            (NSString *)kCVPixelBufferMetalCompatibilityKey: @YES
        }];
        [item addOutput:state.output];
        state.player = [AVPlayer playerWithPlayerItem:item];
        state.player.actionAtItemEnd = AVPlayerActionAtItemEndPause;
        __weak MDPlayer *weakState = state;
        state.observer = [[NSNotificationCenter defaultCenter] addObserverForName:AVPlayerItemDidPlayToEndTimeNotification object:item queue:NSOperationQueue.mainQueue usingBlock:^(NSNotification *note) {
            (void)note;
            MDPlayer *strongState = weakState;
            strongState.ended = YES;
            [strongState.player pause];
        }];
        [state.player play];
        return (__bridge_retained void *)state;
    }
}
void md_player_destroy(void *handle) { if (handle) { MDPlayer *state = (__bridge_transfer MDPlayer *)handle; [state.player pause]; } }
void md_player_replay(void *handle) {
    MDPlayer *state = (__bridge MDPlayer *)handle;
    state.ended = NO;
    __weak MDPlayer *weakState = state;
    [state.player seekToTime:kCMTimeZero toleranceBefore:kCMTimeZero toleranceAfter:kCMTimeZero completionHandler:^(BOOL finished) {
        MDPlayer *strongState = weakState;
        if (finished && strongState && !strongState.ended) [strongState.player play];
    }];
}
void md_player_stop(void *handle) { MDPlayer *state = (__bridge MDPlayer *)handle; state.ended = YES; [state.player pause]; }
int md_player_status(void *handle) {
    MDPlayer *state = (__bridge MDPlayer *)handle;
    if (state.player.status == AVPlayerStatusFailed || state.player.currentItem.status == AVPlayerItemStatusFailed) return -1;
    if (state.ended) return 2;
    return state.player.currentItem.status == AVPlayerItemStatusReadyToPlay ? 1 : 0;
}
CVPixelBufferRef md_player_frame(void *handle) {
    MDPlayer *state = (__bridge MDPlayer *)handle;
    CMTime time = state.player.currentTime;
    if (![state.output hasNewPixelBufferForItemTime:time]) return NULL;
    return [state.output copyPixelBufferForItemTime:time itemTimeForDisplay:NULL];
}

#import <AppKit/AppKit.h>
void md_dark_appearance(void) { NSApplication.sharedApplication.appearance = [NSAppearance appearanceNamed:NSAppearanceNameDarkAqua]; }


int md_probe_video(const char *path, uint32_t *width, uint32_t *height, double *duration) {
    @autoreleasepool {
        NSString *filename = [[NSFileManager defaultManager] stringWithFileSystemRepresentation:path length:strlen(path)];
        if (!filename) return 0;
        AVURLAsset *asset = [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:filename] options:nil];
        AVAssetTrack *track = [asset tracksWithMediaType:AVMediaTypeVideo].firstObject;
        if (!track) return 0;
        CGRect rect = CGRectApplyAffineTransform((CGRect){CGPointZero, track.naturalSize}, track.preferredTransform);
        double seconds = CMTimeGetSeconds(asset.duration);
        if (!isfinite(seconds) || seconds < 0 || rect.size.width == 0 || rect.size.height == 0) return 0;
        *width = (uint32_t)llround(fabs(rect.size.width));
        *height = (uint32_t)llround(fabs(rect.size.height));
        *duration = seconds;
        return 1;
    }
}


#import <ImageIO/ImageIO.h>
int md_video_thumbnail(const char *input, const char *output) {
    @autoreleasepool {
        NSFileManager *files = NSFileManager.defaultManager;
        NSString *source = [files stringWithFileSystemRepresentation:input length:strlen(input)];
        NSString *destination = [files stringWithFileSystemRepresentation:output length:strlen(output)];
        if (!source || !destination) return 0;
        AVURLAsset *asset = [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:source] options:nil];
        AVAssetImageGenerator *generator = [AVAssetImageGenerator assetImageGeneratorWithAsset:asset];
        generator.appliesPreferredTrackTransform = YES;
        generator.maximumSize = CGSizeMake(400, 400);
        CGImageRef frame = [generator copyCGImageAtTime:kCMTimeZero actualTime:NULL error:NULL];
        if (!frame) return 0;
        CGImageDestinationRef writer = CGImageDestinationCreateWithURL((__bridge CFURLRef)[NSURL fileURLWithPath:destination], CFSTR("public.png"), 1, NULL);
        BOOL success = NO;
        if (writer) {
            CGImageDestinationAddImage(writer, frame, NULL);
            success = CGImageDestinationFinalize(writer);
            CFRelease(writer);
        }
        CGImageRelease(frame);
        return success ? 1 : 0;
    }
}
