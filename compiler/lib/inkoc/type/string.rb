# frozen_string_literal: true

module Inkoc
  module Type
    class String
      include Inspect
      include ObjectOperations
      include TypeCompatibility
      include ImmutableType

      def type_name
        'String'
      end
    end
  end
end
