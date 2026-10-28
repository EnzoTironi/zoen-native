#import <Foundation/Foundation.h>
#import "ENRMComposerInputState.h"
#import <assert.h>
#import <math.h>

int main(void) {
  @autoreleasepool {
    ENRMComposerInputState *state = [ENRMComposerInputState new];
    assert([state bind:@"owner-A/normal" richText:YES]);
    NSDictionary *receipt = [state receiptForMarkdown:@"**café 🦊**"];
    assert([receipt[@"text"] isEqual:@"**café 🦊**"]);
    assert([receipt[@"binding"] isEqual:state.binding]);
    assert([receipt[@"eventCount"] unsignedLongLongValue] == 1);
    // A delayed React echo cannot overwrite unacknowledged native formatting.
    assert(![state shouldApplyValue:@"café 🦊" acknowledged:0 current:@"**café 🦊**" force:NO]);
    assert(![state shouldApplyValue:@"**café 🦊**" acknowledged:1 current:@"**café 🦊**" force:NO]);
    assert([state shouldApplyValue:@"" acknowledged:1 current:@"**café 🦊**" force:NO]);
    for (NSNumber *ack in @[@(NAN), @(INFINITY), @(-1), @(0.5), @(9007199254740992.0)])
      assert(![state shouldApplyValue:@"bad" acknowledged:ack.doubleValue current:@"current" force:YES]);
    // Asynchronous link confirmation is exact-operation bound, including later typing.
    assert([state matches:@"owner-A/normal" eventCount:1]);
    [state receiptForMarkdown:@"**café 🦊!**"];
    assert(![state matches:@"owner-A/normal" eventCount:1]);
    state.enabled = NO;
    assert(![state receiptForMarkdown:@"readonly"]);
    assert(![state matches:state.binding eventCount:2]);
    state.enabled = YES;
    assert([state bind:@"owner-B/normal" richText:YES]);
    assert(state.eventCount == 0);
    assert(![state matches:@"owner-A/normal" eventCount:0]);
    assert([state shouldApplyValue:@"B draft" acknowledged:0 current:@"A draft" force:YES]);
    [state receiptForMarkdown:@"B typed"];
    assert([state bind:@"owner-B/normal" richText:NO]);
    assert(state.eventCount == 1); // A mode flag cannot reset admission history.
    assert(![state shouldApplyValue:@"stale mode" acknowledged:0 current:nil force:NO]);
    assert([state shouldApplyValue:@"B typed" acknowledged:1 current:nil force:NO]);
    assert([state bind:@"" richText:YES]);
    assert(![state receiptForMarkdown:@"generic article"]);
    [state bind:@"undo-owner" richText:YES];
    NSUndoManager *undo = [NSUndoManager new];
    undo.groupsByEvent = NO;
    __block NSString *text = @"\\*literal\\* café 🦊";
    __block NSRange selection = NSMakeRange(10, 7);
    __weak __block void (^recursiveRestore)(NSString *, NSRange, NSString *);
    void (^restore)(NSString *, NSRange, NSString *) = ^(NSString *previous, NSRange previousSelection, NSString *binding) {
      assert([binding isEqual:state.binding]);
      [state beginUndoInManager:undo markdown:text selection:selection];
      text = previous; selection = previousSelection;
      [state endUndoWithMarkdown:text composing:NO restore:recursiveRestore];
    };
    recursiveRestore = restore;
    [undo beginUndoGrouping];
    [state beginUndoInManager:undo markdown:text selection:selection];
    assert(!undo.isUndoRegistrationEnabled);
    [state beginUndoInManager:undo markdown:text selection:selection];
    text = @"**literal** café 🦊"; selection = NSMakeRange(0, 7);
    [state endUndoWithMarkdown:text composing:NO restore:restore];
    assert(!undo.isUndoRegistrationEnabled && state.undoDepth == 1);
    [state endUndoWithMarkdown:text composing:NO restore:restore];
    [undo endUndoGrouping];
    assert(undo.isUndoRegistrationEnabled && state.undoDepth == 0 && undo.canUndo);
    [undo undo];
    assert([text isEqual:@"\\*literal\\* café 🦊"] && NSEqualRanges(selection, NSMakeRange(10, 7)));
    [undo redo];
    assert([text isEqual:@"**literal** café 🦊"] && NSEqualRanges(selection, NSMakeRange(0, 7)));
    [undo beginUndoGrouping];
    [state beginUndoInManager:undo markdown:text selection:selection];
    text = @"**literal** café 日本";
    [state endUndoWithMarkdown:text composing:YES restore:restore];
    assert(undo.isUndoRegistrationEnabled);
    [state beginUndoInManager:undo markdown:text selection:selection];
    text = @"**literal** café 日本語";
    [state endUndoWithMarkdown:text composing:NO restore:restore];
    [undo endUndoGrouping];
    [undo undo];
    assert([text isEqual:@"**literal** café 🦊"]);
    [state bind:@"retired-owner" richText:YES];
    NSString *retained = text;
    [undo redo];
    assert([text isEqual:retained]);
    [undo removeAllActions];
    recursiveRestore = nil;
    puts("Composer native receipt PASS");
  }
}
