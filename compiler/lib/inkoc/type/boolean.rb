# frozen_string_literal: true

module Inkoc
  module Type
    class Boolean
      include Inspect
      include ObjectOperations
      include TypeCompatibility
      include ImmutableType
    end
  end
end
