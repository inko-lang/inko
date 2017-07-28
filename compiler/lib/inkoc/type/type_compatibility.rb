# frozen_string_literal: true

module Inkoc
  module Type
    module TypeCompatibility
      # Returns true if the current and the given type are compatible.
      def type_compatible?(other)
        self.class == other.class
      end
    end
  end
end
