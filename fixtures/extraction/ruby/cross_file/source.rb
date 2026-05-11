# Phase 4a fixture: cross-file module call. OtherModule.do_thing is defined
# in another file required as 'other'. The ruby extractor must emit a
# StructuredPendingRelationship with target.terminal_name="do_thing"
# (and ideally target.namespace_path=["OtherModule"] and
# target.import_context="other"). The intra-class self.local_helper resolves.

require 'other'

class Worker
  def entry
    OtherModule.do_thing
    local_helper
  end

  def local_helper
    42
  end
end
