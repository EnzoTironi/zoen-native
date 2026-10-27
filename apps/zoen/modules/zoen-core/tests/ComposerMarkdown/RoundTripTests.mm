#import <Foundation/Foundation.h>
#import "ENRMInputParser.h"
#import "ENRMMarkdownSerializer.h"
#import "ENRMFormattingStore.h"
#import "ENRMBlockStore.h"
#import "ENRMComposerRanges.h"
#import "ENRMComposerInputState.h"
#import "ENRMComposerAvailability.h"

int main() { @autoreleasepool {
  NSError *error=nil;
  NSData *input=[[NSFileHandle fileHandleWithStandardInput] readDataToEndOfFile];
  NSDictionary *request=[NSJSONSerialization JSONObjectWithData:input options:0 error:&error];
  if(error || ![request[@"markdown"] isKindOfClass:NSString.class])return 2;
  BOOL generic=[request[@"generic"] boolValue];
  ENRMInputParser *parser=[ENRMInputParser new];
  ENRMParseResult *parsed=generic ? [parser parseToPlainTextAndRanges:request[@"markdown"]] : [parser parseComposerMarkdown:request[@"markdown"]];
  NSMutableString *text=[parsed.plainText mutableCopy];
  ENRMFormattingStore *styles=[ENRMFormattingStore new];[styles setRanges:parsed.formattingRanges];
  ENRMBlockStore *blocks=[ENRMBlockStore new];[blocks setRanges:parsed.blockRanges];
  BOOL available = YES;
  NSDictionary *probe = request[@"availability"];
  if (probe) {
    NSRange range = NSMakeRange([probe[@"start"] unsignedIntegerValue], [probe[@"length"] unsignedIntegerValue]);
    available = [probe[@"quote"] boolValue]
      ? ENRMComposerCanToggleBlock(text, styles.allRanges, blocks.allRanges, ENRMInputBlockTypeQuote, 1, range)
      : ENRMComposerCanToggle(text, styles.allRanges, (ENRMInputStyleType)[probe[@"type"] integerValue], range, probe[@"url"]);
  }
  __block NSRange selection = NSMakeRange(0, 0);
  NSArray *selected = request[@"selection"];
  if (selected) selection = NSMakeRange([selected[0] unsignedIntegerValue], [selected[1] unsignedIntegerValue]);
  ENRMComposerInputState *state = [ENRMComposerInputState new];
  [state bind:@"roundtrip-owner" richText:YES];
  NSUndoManager *undo = [NSUndoManager new];
  undo.groupsByEvent = NO;
  __weak __block void (^recursiveRestore)(NSString *, NSRange, NSString *);
  void (^restore)(NSString *, NSRange, NSString *) = ^(NSString *value, NSRange oldSelection, NSString *binding) {
    NSString *current = [ENRMMarkdownSerializer serializeComposerText:text ranges:styles.allRanges blockRanges:blocks.allRanges];
    [state beginUndoInManager:undo markdown:current selection:selection];
    ENRMParseResult *restored = [parser parseComposerMarkdown:value];
    [text setString:restored.plainText];
    [styles setRanges:restored.formattingRanges];
    [blocks setRanges:restored.blockRanges];
    selection = oldSelection;
    [state endUndoWithMarkdown:value composing:NO restore:recursiveRestore];
  };
  recursiveRestore = restore;
  [undo beginUndoGrouping];
  NSString *before = [ENRMMarkdownSerializer serializeComposerText:text ranges:styles.allRanges blockRanges:blocks.allRanges];
  [state beginUndoInManager:undo markdown:before selection:selection];
  NSDictionary *edit=request[@"edit"];
  if(edit){NSUInteger start=[edit[@"start"] unsignedIntegerValue],length=[edit[@"delete"] unsignedIntegerValue];
    NSString *value=edit[@"text"];
    if(start>text.length || length>text.length-start || ![value isKindOfClass:NSString.class])return 3;
    [text replaceCharactersInRange:NSMakeRange(start,length) withString:value];
    [styles adjustForEditAtLocation:start deletedLength:length insertedLength:value.length];
    [blocks adjustForEditAtLocation:start deletedLength:length insertedLength:value.length];
    [blocks normalizeToLineBoundsInText:text];
    if (!generic) ENRMRetainComposerLiteralText(styles, text, NSMakeRange(start, value.length));
  }
  NSArray *code=request[@"code"];
  if(code)[styles toggleStyle:ENRMInputStyleTypeCode inRange:NSMakeRange([code[0] unsignedIntegerValue],[code[1] unsignedIntegerValue]) conflictingStyles:NSSet.set];
  for (NSDictionary *toggle in request[@"toggles"]) {
    ENRMInputStyleType type = (ENRMInputStyleType)[toggle[@"type"] integerValue];
    NSRange range = NSMakeRange([toggle[@"start"] unsignedIntegerValue], [toggle[@"length"] unsignedIntegerValue]);
    [styles toggleStyle:type inRange:range
      conflictingStyles:NSSet.set];
    ENRMRetainComposerLiteralText(styles, text, range);
    [styles setRanges:ENRMNormalizeComposerRanges(text, styles.allRanges)];
  }
  [styles setRanges:ENRMNormalizeComposerRanges(text, styles.allRanges)];
  NSArray *quote=request[@"quote"];
  if(quote)[blocks setBlockType:ENRMInputBlockTypeQuote level:1 forParagraphRange:NSMakeRange([quote[0] unsignedIntegerValue],[quote[1] unsignedIntegerValue]) inText:text];
  NSString *markdown=generic ? [ENRMMarkdownSerializer serializePlainText:text ranges:styles.allRanges] :
    [ENRMMarkdownSerializer serializeComposerText:text ranges:styles.allRanges blockRanges:blocks.allRanges];
  [state endUndoWithMarkdown:markdown composing:NO restore:restore];
  [undo endUndoGrouping];
  if ([request[@"undo"] boolValue]) {
    [undo undo];
    markdown = [ENRMMarkdownSerializer serializeComposerText:text ranges:styles.allRanges blockRanges:blocks.allRanges];
  }
  NSMutableArray *types=[NSMutableArray new];for(ENRMFormattingRange *range in styles.allRanges)[types addObject:@(range.type)];
  NSMutableArray *ranges=[NSMutableArray new];
  for (ENRMFormattingRange *range in styles.allRanges)
    [ranges addObject:@{@"type":@(range.type),@"start":@(range.range.location),@"length":@(range.range.length)}];
  NSData *output=[NSJSONSerialization dataWithJSONObject:@{@"plain":text,@"markdown":markdown,@"types":types,@"ranges":ranges,
    @"selection":@[@(selection.location),@(selection.length)],@"available":@(available)} options:0 error:&error];
  [undo removeAllActions]; recursiveRestore = nil;
  if(error)return 4;[[NSFileHandle fileHandleWithStandardOutput] writeData:output];return 0;
}}
