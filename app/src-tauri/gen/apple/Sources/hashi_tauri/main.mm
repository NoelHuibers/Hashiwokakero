#include "bindings/bindings.h"

#import <UIKit/UIKit.h>
#import <objc/runtime.h>

// The app runs on the scene lifecycle (UIApplicationSceneManifest in Info.plist, for the safe
// areas in landscape). There iOS hands the links the app is started with (Universal Links) to
// the scene delegate's scene:willConnectToSession:options:, which tao's TaoSceneDelegate ignores
// (tao 0.37): it passes on only the links of a running app, scene:continueUserActivity: and
// scene:openURLContexts:, to the deep-link plugin. After tao's own method, which starts the app
// (Tauri's setup and its plugins), the launch's links go to those two as well.
static void passOnLaunchLinks(Class delegateClass) {
	static BOOL done = NO;
	SEL willConnect = @selector(scene:willConnectToSession:options:);
	Method method = class_getInstanceMethod(delegateClass, willConnect);
	if (done || !method) {
		return;
	}
	done = YES;
	typedef void (*WillConnect)(id, SEL, UIScene *, UISceneSession *, UISceneConnectionOptions *);
	WillConnect original = (WillConnect)method_getImplementation(method);
	method_setImplementation(method, imp_implementationWithBlock(^(id<UISceneDelegate> delegate, UIScene *scene, UISceneSession *session, UISceneConnectionOptions *options) {
		original(delegate, willConnect, scene, session, options);
		for (NSUserActivity *activity in options.userActivities) {
			if (activity.webpageURL && [delegate respondsToSelector:@selector(scene:continueUserActivity:)]) {
				[delegate scene:scene continueUserActivity:activity];
			}
		}
		if (options.URLContexts.count && [delegate respondsToSelector:@selector(scene:openURLContexts:)]) {
			[delegate scene:scene openURLContexts:options.URLContexts];
		}
	}));
}

// tao names its scene delegate's class when UIKit asks the app for the scene's configuration,
// before the first scene connects: that is when its method can be wrapped
static void watchSceneDelegates(void) {
	SEL setDelegateClass = @selector(setDelegateClass:);
	Method method = class_getInstanceMethod([UISceneConfiguration class], setDelegateClass);
	typedef void (*SetDelegateClass)(id, SEL, Class);
	SetDelegateClass original = (SetDelegateClass)method_getImplementation(method);
	method_setImplementation(method, imp_implementationWithBlock(^(id configuration, Class delegateClass) {
		if (delegateClass) {
			passOnLaunchLinks(delegateClass);
		}
		original(configuration, setDelegateClass, delegateClass);
	}));
}

int main(int argc, char * argv[]) {
	watchSceneDelegates();
	ffi::start_app();
	return 0;
}
