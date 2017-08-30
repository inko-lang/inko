# frozen_string_literal: true

module Inkoc
  module Type
    class Boolean
      include Inspect
      include ObjectOperations
      include TypeCompatibility
      include ImmutableType

      def type_name
        'Boolean'
      end
    end
  end
end
