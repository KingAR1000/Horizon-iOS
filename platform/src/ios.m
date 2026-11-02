#import <UIKit/UIKit.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>
#import <AVFoundation/AVFoundation.h>
#import <Security/Security.h>

// UIKit and Rust consume this queue on the same main thread.
typedef struct { int kind, code; double x, y; } HZInput;
static HZInput events[512];
static unsigned head, tail;
static NSString *returned;
static BOOL setupRequested;
static UIBackgroundTaskIdentifier backgroundTask;
static void emit(int kind, int code, double x, double y) {
    unsigned next = (head + 1) % 512;
    if (next == tail) { tail = head = 0; events[head++] = (HZInput){5,0,0,0}; }
    events[head] = (HZInput){kind,code,x,y}; head = (head+1)%512;
}
bool hz_poll(HZInput *event) {
    if (tail == head) return false;
    *event = events[tail]; tail = (tail+1)%512; return true;
}
const char *hz_documents(void) {
    returned = NSSearchPathForDirectoriesInDomains(NSDocumentDirectory, NSUserDomainMask, YES).firstObject;
    return returned.UTF8String;
}
const char *hz_clipboard_get(void) { returned = UIPasteboard.generalPasteboard.string ?: @""; return returned.UTF8String; }
void hz_clipboard_set(const char *text) { UIPasteboard.generalPasteboard.string = [NSString stringWithUTF8String:text]; }
void hz_background(bool begin) {
    static dispatch_once_t once;
    dispatch_once(&once, ^{ backgroundTask = UIBackgroundTaskInvalid; });
    if (begin && backgroundTask == UIBackgroundTaskInvalid) {
        backgroundTask = [UIApplication.sharedApplication beginBackgroundTaskWithExpirationHandler:^{
            if (backgroundTask != UIBackgroundTaskInvalid) {
                [UIApplication.sharedApplication endBackgroundTask:backgroundTask]; backgroundTask = UIBackgroundTaskInvalid;
            }
        }];
    } else if (!begin && backgroundTask != UIBackgroundTaskInvalid) {
        [UIApplication.sharedApplication endBackgroundTask:backgroundTask]; backgroundTask = UIBackgroundTaskInvalid;
    }
}

@interface HZOverlay : UIView <UIKeyInput, UIDocumentPickerDelegate>
@property BOOL playing;
@property BOOL importing;
@property NSMutableArray<UIButton *> *buttons;
@property UILabel *message;
@property UIView *look;
@property UIView *stick;
@property CGPoint origin;
@property int movement;
@end
static HZOverlay *overlay;
@implementation HZOverlay
- (BOOL)canBecomeFirstResponder { return YES; }
- (BOOL)hasText { return YES; }
- (UIKeyboardType)keyboardType { return UIKeyboardTypeASCIICapable; }
- (UITextAutocorrectionType)autocorrectionType { return UITextAutocorrectionTypeNo; }
- (void)insertText:(NSString *)text {
    [(id<UIKeyInput>)self.superview insertText:text];
    if ([text containsString:@"\n"]) [self resignFirstResponder];
}
- (void)deleteBackward { [(id<UIKeyInput>)self.superview deleteBackward]; }
- (UIView *)hitTest:(CGPoint)p withEvent:(UIEvent *)event {
    UIView *hit = [super hitTest:p withEvent:event];
    return hit == self ? nil : hit;
}
- (UIButton *)button:(NSString *)title code:(int)code {
    UIButton *b = [UIButton buttonWithType:UIButtonTypeSystem];
    [b setTitle:NSLocalizedString(title,nil) forState:UIControlStateNormal];
    [b setTitleColor:UIColor.whiteColor forState:UIControlStateNormal];
    b.backgroundColor = [UIColor colorWithWhite:0 alpha:0.48];
    b.layer.cornerRadius = 12; b.tag = code;
    b.exclusiveTouch = NO;
    [b addTarget:self action:@selector(down:) forControlEvents:UIControlEventTouchDown];
    [b addTarget:self action:@selector(up:) forControlEvents:UIControlEventTouchUpInside|UIControlEventTouchUpOutside|UIControlEventTouchCancel];
    [self addSubview:b]; [self.buttons addObject:b]; return b;
}
- (void)down:(UIButton *)b {
    if (b.tag == 100) { if (self.isFirstResponder) [self resignFirstResponder]; else [self becomeFirstResponder]; return; }
    if (b.tag == 102) { setupRequested=YES; return; }
    if (b.tag == 101) { [self importResources]; return; }
    emit(b.tag >= 20 ? 2 : 1, (int)b.tag,1,0);
}
- (void)up:(UIButton *)b { if (b.tag < 100) emit(b.tag >= 20 ? 2 : 1,(int)b.tag,0,0); }
- (instancetype)initWithFrame:(CGRect)frame {
    if ((self=[super initWithFrame:frame])) {
        self.multipleTouchEnabled = YES;
        self.buttons = [NSMutableArray new];
        self.look = [UIView new]; self.look.multipleTouchEnabled = YES;
        [self.look addGestureRecognizer:[[UIPanGestureRecognizer alloc] initWithTarget:self action:@selector(pan:)]];
        [self addSubview:self.look];
        self.stick = [UIView new]; self.stick.backgroundColor=[UIColor colorWithWhite:1 alpha:0.16]; self.stick.layer.cornerRadius=60;
        [self.stick addGestureRecognizer:[[UIPanGestureRecognizer alloc] initWithTarget:self action:@selector(move:)]];
        [self addSubview:self.stick];
        for (NSArray *spec in @[@[@"Jump",@4],@[@"Sneak",@5],@[@"Sprint",@6],@[@"Break",@20],@[@"Use",@21],@[@"Inventory",@7],@[@"Pause",@8],@[@"Chat",@9],@[@"‹",@22],@[@"›",@23],@[@"Drop",@10],@[@"Keyboard",@100]])
            [self button:spec[0] code:[spec[1] intValue]];
        [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(reset) name:UIApplicationWillResignActiveNotification object:nil];
        [[AVAudioSession sharedInstance] setCategory:AVAudioSessionCategoryAmbient error:nil];
        [[AVAudioSession sharedInstance] setActive:YES error:nil];
    } return self;
}
- (void)reset { emit(5,0,0,0); self.movement=0; [self resignFirstResponder]; }
- (void)pan:(UIPanGestureRecognizer *)g {
    CGPoint d=[g translationInView:self]; [g setTranslation:CGPointZero inView:self];
    if (self.playing) emit(3,0,d.x*2,d.y*2);
}
- (void)move:(UIPanGestureRecognizer *)g {
    if (g.state == UIGestureRecognizerStateBegan) self.origin=[g locationInView:self.stick];
    CGPoint p=[g locationInView:self.stick]; double x=p.x-self.origin.x, y=p.y-self.origin.y;
    int mask = (x < -12 ? 1 : 0)|(x > 12 ? 2 : 0)|(y < -12 ? 4 : 0)|(y > 12 ? 8 : 0);
    if (g.state == UIGestureRecognizerStateEnded || g.state == UIGestureRecognizerStateCancelled) mask=0;
    int codes[]={0,1,2,3};
    for (int i=0;i<4;i++) if ((mask & (1<<i)) != (self.movement & (1<<i))) emit(1,codes[i],!!(mask&(1<<i)),0);
    self.movement=mask;
}
- (void)layoutSubviews {
    [super layoutSubviews];
    CGRect r=UIEdgeInsetsInsetRect(self.bounds,self.safeAreaInsets);
    CGFloat left=r.origin.x+16, right=CGRectGetMaxX(r)-16, top=r.origin.y+8, bottom=CGRectGetMaxY(r)-12;
    self.look.frame=CGRectMake(CGRectGetMidX(r),top,r.size.width/2,r.size.height-80);
    self.stick.frame=CGRectMake(left,bottom-145,120,120);
    for (UIButton *b in self.buttons) {
        b.hidden = self.importing ? (b.tag != 101 && b.tag != 102) : (!self.playing && b.tag != 8 && b.tag != 100 && b.tag != 21 && b.tag != 102);
        if (b.tag == 102 && self.playing) b.hidden=YES;
        switch (b.tag) {
            case 4:b.frame=CGRectMake(right-70,bottom-70,70,60);break;
            case 5:b.frame=CGRectMake(right-150,bottom-70,70,60);break;
            case 6:b.frame=CGRectMake(left+130,bottom-65,65,55);break;
            case 20:b.frame=CGRectMake(right-70,bottom-205,70,55);break;
            case 21:b.frame=CGRectMake(right-70,bottom-140,70,55);break;
            case 7:b.frame=CGRectMake(right-85,top,85,40);break;
            case 8:b.frame=CGRectMake(left,top,65,40);break;
            case 9:b.frame=CGRectMake(left+75,top,60,40);break;
            case 10:b.frame=CGRectMake(right-230,bottom-65,65,55);break;
            case 22:b.frame=CGRectMake(CGRectGetMidX(r)-105,bottom-45,45,40);break;
            case 23:b.frame=CGRectMake(CGRectGetMidX(r)+60,bottom-45,45,40);break;
            case 100:b.frame=CGRectMake(left+145,top,90,40);break;
            case 102:b.frame=CGRectMake(CGRectGetMidX(r)-125,self.importing ? CGRectGetMidY(r)+100 : top,250,44);break;
            case 101:b.frame=CGRectMake(CGRectGetMidX(r)-125,CGRectGetMidY(r)+40,250,50);break;
        }
    }
    self.stick.hidden = self.look.hidden = !self.playing || self.importing;
    self.message.frame=CGRectMake(left,top+55,r.size.width-32,r.size.height/2-40);
}
- (UIViewController *)controller {
    return self.window.rootViewController;
}
- (void)importResources {
    UIDocumentPickerViewController *picker=[[UIDocumentPickerViewController alloc] initForOpeningContentTypes:@[UTTypeFolder] asCopy:NO];
    picker.delegate=self; picker.allowsMultipleSelection=NO;
    [[self controller] presentViewController:picker animated:YES completion:nil];
}
- (void)documentPicker:(UIDocumentPickerViewController *)picker didPickDocumentsAtURLs:(NSArray<NSURL *> *)urls {
    NSURL *url=urls.firstObject; if (!url) return;
    self.message.text=NSLocalizedString(@"Importing resources…",nil);
    BOOL scoped=[url startAccessingSecurityScopedResource];
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED,0), ^{
        NSFileManager *fm=NSFileManager.defaultManager;
        NSURL *docs=[fm URLsForDirectory:NSDocumentDirectory inDomains:NSUserDomainMask].firstObject;
        NSURL *stage=[docs URLByAppendingPathComponent:@"resources-import"];
        NSURL *dest=[docs URLByAppendingPathComponent:@"resources"];
        __block NSError *error=nil;
        [fm removeItemAtURL:stage error:nil];
        NSFileCoordinator *coordinator=[[NSFileCoordinator alloc] initWithFilePresenter:nil];
        [coordinator coordinateReadingItemAtURL:url options:0 error:&error byAccessor:^(NSURL *source) {
            NSData *data=[NSData dataWithContentsOfURL:[source URLByAppendingPathComponent:@"horizon-resources.json"]];
            if (!data || ![NSJSONSerialization JSONObjectWithData:data options:0 error:&error]) {
                if (!error) error=[NSError errorWithDomain:@"Horizon" code:1 userInfo:@{NSLocalizedDescriptionKey:NSLocalizedString(@"Select the prepared resource folder containing horizon-resources.json.",nil)}];
                return;
            }
            [fm copyItemAtURL:source toURL:stage error:&error];
        }];
        if (!error) {
            if ([fm fileExistsAtPath:dest.path]) [fm replaceItemAtURL:dest withItemAtURL:stage backupItemName:nil options:0 resultingItemURL:nil error:&error];
            else [fm moveItemAtURL:stage toURL:dest error:&error];
        }
        if (scoped) [url stopAccessingSecurityScopedResource];
        if (error) [fm removeItemAtURL:stage error:nil];
        dispatch_async(dispatch_get_main_queue(), ^{
            self.message.text=error ? error.localizedDescription : NSLocalizedString(@"Import complete. Close Horizon from the app switcher and reopen it to play.",nil);
        });
    });
}
@end
void hz_attach(void *view, bool importer) {
    UIView *parent=(__bridge UIView *)view;
    if (overlay.superview == parent) return;
    [overlay removeFromSuperview];
    overlay=[[HZOverlay alloc] initWithFrame:parent.bounds];
    overlay.autoresizingMask=UIViewAutoresizingFlexibleWidth|UIViewAutoresizingFlexibleHeight;
    overlay.importing=importer;
    [overlay button:@"Sign in with Microsoft" code:102];
    if (importer) {
        overlay.backgroundColor=[UIColor colorWithRed:0.07 green:0.10 blue:0.15 alpha:1];
        overlay.message=[UILabel new]; overlay.message.numberOfLines=0; overlay.message.textColor=UIColor.whiteColor; overlay.message.textAlignment=NSTextAlignmentCenter;
        overlay.message.text=NSLocalizedString(@"Horizon needs resources prepared from your own Minecraft installation. Select the exported resource folder in Files.",nil);
        [overlay addSubview:overlay.message]; [overlay button:@"Import resources" code:101];
    }
    if (!overlay.message) {
        overlay.message=[UILabel new]; overlay.message.numberOfLines=0; overlay.message.textColor=UIColor.whiteColor;
        overlay.message.backgroundColor=[UIColor colorWithWhite:0 alpha:0.65]; overlay.message.hidden=YES;
        [overlay addSubview:overlay.message];
    }
    [parent addSubview:overlay]; [overlay setNeedsLayout];
}
void hz_mode(bool playing) {
    if (overlay.playing != playing) { overlay.playing=playing; [overlay reset]; [overlay setNeedsLayout]; }
}
bool hz_active(void) { return UIApplication.sharedApplication.applicationState == UIApplicationStateActive; }

bool hz_setup_requested(void) { BOOL value=setupRequested; setupRequested=NO; return value; }
void hz_setup_status(const char *text) {
    NSString *message=[NSString stringWithUTF8String:text];
    dispatch_async(dispatch_get_main_queue(), ^{ overlay.message.hidden=NO; overlay.message.text=NSLocalizedString(message,nil); });
}
void hz_setup_device(const char *code, const char *uri) {
    NSString *message=[NSString stringWithFormat:NSLocalizedString(@"Enter code %@ at microsoft.com/link",nil),[NSString stringWithUTF8String:code]];
    NSURL *url=[NSURL URLWithString:[NSString stringWithUTF8String:uri]];
    dispatch_async(dispatch_get_main_queue(), ^{
        overlay.message.hidden=NO; overlay.message.text=message;
        if ([url.scheme isEqualToString:@"https"] && [url.host isEqualToString:@"www.microsoft.com"]) {
            UIAlertController *alert=[UIAlertController alertControllerWithTitle:NSLocalizedString(@"Microsoft sign-in",nil) message:message preferredStyle:UIAlertControllerStyleAlert];
            [alert addAction:[UIAlertAction actionWithTitle:NSLocalizedString(@"Continue in browser",nil) style:UIAlertActionStyleDefault handler:^(UIAlertAction *action) {
                [UIApplication.sharedApplication openURL:url options:@{} completionHandler:nil];
            }]];
            [overlay.controller presentViewController:alert animated:YES completion:nil];
        }
    });
}
static NSDictionary *secretQuery(void) { return @{(__bridge id)kSecClass:(__bridge id)kSecClassGenericPassword,(__bridge id)kSecAttrService:@"io.github.kingar1000.horizon.account",(__bridge id)kSecAttrAccount:@"microsoft"}; }
const char *hz_secret_get(void) {
    NSMutableDictionary *query=[secretQuery() mutableCopy]; query[(__bridge id)kSecReturnData]=@YES;
    CFTypeRef value=NULL;
    if (SecItemCopyMatching((__bridge CFDictionaryRef)query,&value)!=errSecSuccess) return NULL;
    NSData *data=CFBridgingRelease(value); returned=[[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding]; return returned.UTF8String;
}
bool hz_secret_set(const char *text) {
    NSData *data=[[NSString stringWithUTF8String:text] dataUsingEncoding:NSUTF8StringEncoding];
    NSDictionary *attributes=@{(__bridge id)kSecValueData:data};
    OSStatus result=SecItemUpdate((__bridge CFDictionaryRef)secretQuery(),(__bridge CFDictionaryRef)attributes);
    if (result==errSecItemNotFound) {
        NSMutableDictionary *query=[secretQuery() mutableCopy]; [query addEntriesFromDictionary:attributes];
        query[(__bridge id)kSecAttrAccessible]=(__bridge id)kSecAttrAccessibleWhenUnlockedThisDeviceOnly;
        result=SecItemAdd((__bridge CFDictionaryRef)query,NULL);
    }
    return result==errSecSuccess;
}
