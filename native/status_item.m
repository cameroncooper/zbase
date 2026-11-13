// The menu bar icon. Clicking it hands the icon's place on screen to Rust,
// which shows the recent-conversations panel under it; while the panel is
// open, a click anywhere else asks Rust to close it.
#import <AppKit/AppKit.h>

typedef void (*ZBaseStatusItemClick)(uint32_t display, double x, double y, double width, double height);
typedef void (*ZBaseStatusItemOutsideClick)(void);

@interface ZBaseStatusItemTarget : NSObject
@property ZBaseStatusItemClick onClick;
- (void)clicked:(id)sender;
@end

static NSStatusItem *item;
static ZBaseStatusItemTarget *target;
static uint32_t unreadCount;
static ZBaseStatusItemOutsideClick onOutsideClick;
static id globalMonitor;
static id localMonitor;

@implementation ZBaseStatusItemTarget
- (void)clicked:(id)sender {
    (void)sender;
    NSWindow *window = item.button.window;
    NSScreen *screen = window.screen ?: NSScreen.mainScreen;
    if (!window || !screen || !self.onClick) {
        return;
    }
    NSRect icon = [window convertRectToScreen:[item.button convertRect:item.button.bounds toView:nil]];
    NSRect frame = screen.frame;
    uint32_t display = [screen.deviceDescription[@"NSScreenNumber"] unsignedIntValue];
    // Rust places windows from the top-left of their display, y growing down.
    self.onClick(display, NSMinX(icon) - NSMinX(frame), NSMaxY(frame) - NSMaxY(icon), NSWidth(icon),
                 NSHeight(icon));
}
@end

static NSImage *appIcon;

// zBase's icon, with the unread count in a red badge on its top-right corner
// as on the Dock. Drawn on demand, so it stays sharp at any screen scale.
static NSImage *icon_image(uint32_t count) {
    // Nearly as tall as the menu bar (22pt, or 24pt beside a notch); the badge
    // keeps its share of the icon.
    const CGFloat iconSide = floor(NSStatusBar.systemStatusBar.thickness * 0.9);
    const CGFloat badgeHeight = round(iconSide * 0.6), badgeInset = round(iconSide * 0.15);
    NSString *label = count > 0 ? [NSString stringWithFormat:@"%u", count] : nil;
    NSDictionary *attributes = @{
        NSFontAttributeName : [NSFont systemFontOfSize:round(iconSide * 0.45) weight:NSFontWeightBold],
        NSForegroundColorAttributeName : NSColor.whiteColor,
    };
    NSSize text = label ? [label sizeWithAttributes:attributes] : NSZeroSize;
    // A circle for one digit, a pill for more.
    CGFloat badgeWidth = label ? MAX(badgeHeight, ceil(text.width) + 5) : 0;
    // The badge overhangs the icon's right edge, like the Dock's.
    CGFloat badgeX = iconSide - badgeWidth + badgeInset;
    NSSize canvas = NSMakeSize(MAX(iconSide, badgeX + badgeWidth), iconSide);
    NSImage *image = [NSImage imageWithSize:canvas
                                    flipped:NO
                             drawingHandler:^BOOL(NSRect rect) {
                                 (void)rect;
                                 [appIcon drawInRect:NSMakeRect(0, 0, iconSide, iconSide)];
                                 if (!label) {
                                     return YES;
                                 }
                                 NSRect badge = NSMakeRect(badgeX, iconSide - badgeHeight, badgeWidth, badgeHeight);
                                 [NSColor.systemRedColor setFill];
                                 [[NSBezierPath bezierPathWithRoundedRect:badge
                                                                  xRadius:badgeHeight / 2
                                                                  yRadius:badgeHeight / 2] fill];
                                 [label drawAtPoint:NSMakePoint(NSMidX(badge) - text.width / 2,
                                                                NSMidY(badge) - text.height / 2)
                                     withAttributes:attributes];
                                 return YES;
                             }];
    image.accessibilityDescription =
        label ? [NSString stringWithFormat:@"zBase, %@ unread", label] : @"zBase";
    return image;
}

// `icon_png` is zBase's icon; the bundle's icon stands in if it can't be read.
// The icon appears once `zbase_status_item_set_visible` asks for it.
void zbase_status_item_start(ZBaseStatusItemClick on_click, ZBaseStatusItemOutsideClick on_outside_click,
                             const uint8_t *icon_png, size_t icon_len) {
    if (target) {
        return;
    }
    appIcon = [[NSImage alloc] initWithData:[NSData dataWithBytes:icon_png length:icon_len]]
                  ?: NSApp.applicationIconImage;
    target = [ZBaseStatusItemTarget new];
    target.onClick = on_click;
    onOutsideClick = on_outside_click;
}

void zbase_status_item_watch_outside_clicks(bool watch);

// Adds the icon to the menu bar or removes it. Removing, rather than hiding,
// leaves no visibility for macOS to remember between launches.
void zbase_status_item_set_visible(bool visible) {
    if (!target) {
        return;
    }
    if (!visible) {
        zbase_status_item_watch_outside_clicks(false);
        if (item) {
            [NSStatusBar.systemStatusBar removeStatusItem:item];
            item = nil;
        }
        return;
    }
    if (item) {
        return;
    }
    item = [NSStatusBar.systemStatusBar statusItemWithLength:NSVariableStatusItemLength];
    item.button.image = icon_image(unreadCount);
    item.button.toolTip = @"zBase";
    item.button.target = target;
    item.button.action = @selector(clicked:);
}

// Shows `count`, the Dock badge's number, on the icon; 0 shows the icon alone.
// Kept while the icon is off, so it is current when the icon appears.
void zbase_status_item_set_unread(uint32_t count) {
    unreadCount = count;
    if (!item) {
        return;
    }
    item.button.image = icon_image(count);
}

// While the panel is open, report presses outside zBase and on zBase's other
// windows; a press on the icon itself is left to its action, which toggles.
void zbase_status_item_watch_outside_clicks(bool watch) {
    if (!item) {
        return;
    }
    if (!watch) {
        if (globalMonitor) {
            [NSEvent removeMonitor:globalMonitor];
            globalMonitor = nil;
        }
        if (localMonitor) {
            [NSEvent removeMonitor:localMonitor];
            localMonitor = nil;
        }
        return;
    }
    if (globalMonitor) {
        return;
    }
    NSEventMask presses = NSEventMaskLeftMouseDown | NSEventMaskRightMouseDown | NSEventMaskOtherMouseDown;
    globalMonitor = [NSEvent addGlobalMonitorForEventsMatchingMask:presses
                                                           handler:^(NSEvent *event) {
                                                               (void)event;
                                                               onOutsideClick();
                                                           }];
    localMonitor = [NSEvent addLocalMonitorForEventsMatchingMask:presses
                                                         handler:^NSEvent *(NSEvent *event) {
                                                             // Pop-up windows, the panel among them, are NSPanels.
                                                             if (event.window != item.button.window &&
                                                                 ![event.window isKindOfClass:NSPanel.class]) {
                                                                 onOutsideClick();
                                                             }
                                                             return event;
                                                         }];
}
