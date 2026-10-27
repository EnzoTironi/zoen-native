#import <Foundation/Foundation.h>
#import "ENRMInputParser.h"
#import "ENRMMarkdownSerializer.h"
#import "ENRMFormattingStore.h"
#import "ENRMBlockStore.h"
#import "ENRMComposerInputState.h"
#import <assert.h>

// UIKit clipboard/selection/replacement are ports. The runner inserts the
// unchanged production paste:, pasteMarkdown: and currentMarkdown methods.
@interface SelectionPort : NSObject
@property(nonatomic) NSRange selectedRange;
@property(nonatomic, strong) NSString *text;
@end
@implementation SelectionPort
@end
static NSString *ENRMGetPlainText(SelectionPort *view) { return view.text; }
static NSString *const kENRMMarkdownPasteboardType = @"com.swmansion.enriched-markdown.markdown";

@interface UIPasteboard : NSObject
@property(nonatomic, copy) NSString *string;
@property(nonatomic, strong) id markdown;
+ (instancetype)generalPasteboard;
- (id)valueForPasteboardType:(NSString *)type;
@end
@implementation UIPasteboard
+ (instancetype)generalPasteboard {
  static UIPasteboard *board;
  if (!board) board = [UIPasteboard new];
  return board;
}
- (id)valueForPasteboardType:(NSString *)type { return self.markdown; }
@end

@interface PasteHost : NSObject {
  SelectionPort *_textView;
  ENRMComposerInputState *_composer;
  ENRMFormattingStore *_formattingStore;
  ENRMBlockStore *_blockStore;
  NSMutableString *_text;
}
@property(nonatomic, copy) NSString *lastEvent;
- (void)bind:(NSString *)binding rich:(BOOL)rich value:(NSString *)value;
- (void)pasteMarkdown:(NSString *)markdown;
- (NSString *)canonical;
- (NSString *)visible;
- (BOOL)isComposerRichInput;
- (BOOL)isComposerPlainInput;
- (NSString *)currentMarkdown;
- (NSArray *)allRangesIncludingTransient;
- (NSString *)serializeText:(NSString *)text ranges:(NSArray *)ranges blockRanges:(NSArray *)blocks;
- (void)replaceTextInRange:(NSRange)range withText:(NSString *)text
        formattingRanges:(NSArray *)ranges blockRanges:(NSArray *)blocks;
@end
@implementation PasteHost
- (instancetype)init {
  if ((self = [super init])) {
    _textView = [SelectionPort new];
    _composer = [ENRMComposerInputState new];
    _formattingStore = [ENRMFormattingStore new];
    _blockStore = [ENRMBlockStore new];
  }
  return self;
}
- (void)bind:(NSString *)binding rich:(BOOL)rich value:(NSString *)value {
  [_composer bind:binding richText:rich];
  ENRMParseResult *parsed = rich ? [[ENRMInputParser new] parseComposerMarkdown:value] : nil;
  _text = [(rich ? parsed.plainText : value) mutableCopy];
  [_formattingStore setRanges:rich ? parsed.formattingRanges : @[]];
  [_blockStore setRanges:rich ? parsed.blockRanges : @[]];
  _textView.text = _text;
  _textView.selectedRange = NSMakeRange(_text.length, 0);
}
- (BOOL)isComposerRichInput { return _composer.binding.length && _composer.richText; }
- (BOOL)isComposerPlainInput { return _composer.binding.length && !_composer.richText; }
- (NSString *)canonical { return [self currentMarkdown]; }
- (NSString *)visible { return _text; }
- (NSArray *)allRangesIncludingTransient { return _formattingStore.allRanges; }
- (NSString *)serializeText:(NSString *)text ranges:(NSArray *)ranges blockRanges:(NSArray *)blocks {
  NSAssert(NO, @"These cases must use the actual composer branch");
  return @"";
}
- (void)replaceTextInRange:(NSRange)range withText:(NSString *)text
        formattingRanges:(NSArray *)ranges blockRanges:(NSArray *)blocks {
  [_text replaceCharactersInRange:range withString:text];
  [_formattingStore adjustForEditAtLocation:range.location deletedLength:range.length insertedLength:text.length];
  [_blockStore adjustForEditAtLocation:range.location deletedLength:range.length insertedLength:text.length];
  for (ENRMFormattingRange *r in ranges) {
    [_formattingStore addRange:[ENRMFormattingRange rangeWithType:r.type
      range:NSMakeRange(r.range.location + range.location, r.range.length) url:r.url]];
  }
  for (ENRMBlockRange *r in blocks) {
    [_blockStore setBlockType:r.type level:r.level
      forParagraphRange:NSMakeRange(r.range.location + range.location, r.range.length) inText:_text];
  }
  _textView.selectedRange = NSMakeRange(range.location + text.length, 0);
  self.lastEvent = [_composer receiptForMarkdown:[self canonical]][@"text"];
}
#include "PasteMethod.inc"
#include "CurrentMethod.inc"
@end

@interface InputActionPort : NSObject
@property(nonatomic, strong) PasteHost *markdownTextInput;
@property(nonatomic, strong) id selectedTextRange;
- (void)paste:(id)sender;
- (void)replaceRange:(id)range withText:(NSString *)text;
@end
@implementation InputActionPort
- (void)replaceRange:(id)range withText:(NSString *)text {
  NSAssert(NO, @"The real input must retain its editor owner");
}
#include "PasteAction.inc"
@end

int main() { @autoreleasepool {
  PasteHost *host = [PasteHost new];
  InputActionPort *input = [InputActionPort new];
  input.markdownTextInput = host;
  UIPasteboard *board = UIPasteboard.generalPasteboard;
  for (NSString *text in @[@"**termo**", @"[rótulo](https://example.org)", @"\n**café 🦊**\n"]) {
    [host bind:@"plain-A" rich:NO value:@""];
    board.string = text;
    board.markdown = nil;
    [input paste:nil];
    assert([[host visible] isEqual:text]);
    assert([[host canonical] isEqual:text] && [host.lastEvent isEqual:text]);
  }
  [host bind:@"plain-A" rich:NO value:@""];
  board.string = @"termo";
  board.markdown = @"**termo**";
  [input paste:nil];
  assert([[host visible] isEqual:@"termo"]); // Plain clipboard wins in search.
  NSString *query = @"**termo** [rótulo](https://example.org)";
  [host bind:@"plain-A" rich:NO value:query];
  [host bind:@"rich-B" rich:YES value:@"**visual** "];
  board.string = @"rótulo";
  board.markdown = @"[rótulo](https://example.org)";
  [input paste:nil];
  assert([[host visible] isEqual:@"visual rótulo"]);
  assert([[host canonical] isEqual:@"**visual** [rótulo](https://example.org)"]);
  [host bind:@"plain-C" rich:NO value:query];
  [host pasteMarkdown:@" **novo**"];
  assert([[host visible] isEqual:[query stringByAppendingString:@" **novo**"]]);
  assert([host.lastEvent isEqual:[host canonical]]);
  puts("Actual native paste method PASS");
} }
