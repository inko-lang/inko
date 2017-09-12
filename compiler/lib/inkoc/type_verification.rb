# frozen_string_literal: true

module Inkoc
  module TypeVerification
    def ensure_compatible_types(nodes, types)
      return if types.length <= 1

      first_type = types[0]

      types.each_with_index do |type, index|
        next if type.type_compatible?(first_type)

        location = nodes[index].location

        diagnostics.type_error(first_type, type, location)
      end
    end
  end
end
