# frozen_string_literal: true

module Inkoc
  module Equality
    def ==(other)
      return false unless other.is_a?(self.class)

      ours = cached_instance_variables
      theirs = other.cached_instance_variables

      return false unless ours.length == theirs.length

      ours.each_with_index do |our, index|
        their = theirs[index]

        if instance_variable_get(our) != other.instance_variable_get(their)
          return false
        end
      end

      true
    end

    def cached_instance_variables
      @cached_instance_variables ||= instance_variables
    end
  end
end
