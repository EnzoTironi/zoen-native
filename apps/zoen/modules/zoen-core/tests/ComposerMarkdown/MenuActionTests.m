#import <Foundation/Foundation.h>
#import "ENRMComposerInputState.h"
#import <assert.h>

// UIKit readiness is a port; the captured action method and receipt state are
// production code. This does not qualify UIAction delivery on an iPhone.
@interface ComposerSelectionPort : NSObject
@property NSRange selectedRange;
@end
@implementation ComposerSelectionPort
@end

@interface EnrichedMarkdownTextInput : NSObject {
@public
  ENRMComposerInputState *_composer;
  ComposerSelectionPort *_textView;
}
@property NSString *markdown;
@property BOOL ready;
- (dispatch_block_t)composerMenuAction:(dispatch_block_t)work;
@end

@implementation EnrichedMarkdownTextInput
- (instancetype)init {
  if ((self = [super init])) {
    _composer = [ENRMComposerInputState new];
    _textView = [ComposerSelectionPort new];
    _textView.selectedRange = NSMakeRange(1, 2);
    self.markdown = @"**café 🦊**";
    self.ready = YES;
  }
  return self;
}
- (BOOL)composerMenuMayAct { return self.ready; }
- (NSString *)currentMarkdown { return self.markdown; }
- (void)performComposerFormatting:(dispatch_block_t)work { work(); }
#include "ComposerMenuAction.inc"
@end

int main(void) {
  @autoreleasepool {
    EnrichedMarkdownTextInput *view = [EnrichedMarkdownTextInput new];
    [view->_composer bind:@"owner-A" richText:YES];
    [view->_composer receiptForMarkdown:view.markdown];
    __block NSUInteger effects = 0;
    dispatch_block_t old = [view composerMenuAction:^{ effects++; }];
    old(); assert(effects == 1);
    // Fresh owner at the same revision and same text cannot borrow the action.
    [view->_composer bind:@"owner-B" richText:YES];
    [view->_composer receiptForMarkdown:view.markdown];
    old(); assert(effects == 1);
    dispatch_block_t typed = [view composerMenuAction:^{ effects++; }];
    [view->_composer receiptForMarkdown:view.markdown];
    typed(); assert(effects == 1);
    dispatch_block_t selected = [view composerMenuAction:^{ effects++; }];
    view->_textView.selectedRange = NSMakeRange(0, 1);
    selected(); assert(effects == 1);
    dispatch_block_t replaced = [view composerMenuAction:^{ effects++; }];
    view.markdown = @"another controlled draft";
    replaced(); assert(effects == 1);
    dispatch_block_t blurred = [view composerMenuAction:^{ effects++; }];
    view.ready = NO;
    blurred(); assert(effects == 1);
    view.ready = YES;
    view->_composer.enabled = NO;
    blurred(); assert(effects == 1);
    view->_composer.enabled = YES;
    dispatch_block_t active = [view composerMenuAction:^{ effects++; }];
    active(); assert(effects == 2);
    // No composer binding/ACK: only the dispatch bypass is exercised here.
    // The article work itself retains its original draft checkpoint authority.
    [view->_composer bind:@"" richText:YES];
    view.ready = NO;
    view->_composer.enabled = NO;
    dispatch_block_t article = [view composerMenuAction:^{ effects++; }];
    article(); assert(effects == 3);
    [view->_composer bind:@"new-composer" richText:YES];
    view.ready = YES;
    view->_composer.enabled = YES;
    article(); assert(effects == 3);
    puts("Production composer menu action PASS");
  }
}
