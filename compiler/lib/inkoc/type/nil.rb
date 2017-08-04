# frozen_string_literal: true

module Inkoc
  module Type
    class Nil
      include Inspect
      include ObjectOperations
      include TypeCompatibility
      include ImmutableType
    end
  end
end
